//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! AT-interface connection service – manages up to 6 TCP/UDP/SSL connections.

use crate::ssl::AtSslConfig;
use crate::data_mode::{DataModeFinish, DataModeSession};
use anyhow::Result;
use log::error;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{LazyLock, Mutex};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};

const MAX_CONNECTIONS: usize = 6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionKind {
    Tcp,
    Udp,
    Ssl,
}

enum ConnStream {
    Tcp(TcpStream),
    Tls(Box<tokio_openssl::SslStream<TcpStream>>),
    Udp(UdpSocket),
}

struct Connection {
    kind: ConnectionKind,
    addr: String,
    ssl_config: Option<AtSslConfig>,
    stream: Option<ConnStream>,
}

pub struct ConnectionService {
    connections: HashMap<usize, Connection>,
}

static INSTANCE: LazyLock<Mutex<ConnectionService>> = LazyLock::new(|| {
    Mutex::new(ConnectionService {
        connections: HashMap::new(),
    })
});

impl ConnectionService {
    pub fn instance() -> &'static Mutex<ConnectionService> {
        &INSTANCE
    }

    pub async fn start_connection(
        id: usize,
        conn_type: &str,
        addr: &str,
        port: &str,
        keepalive: u32,
    ) -> bool {
        if id >= MAX_CONNECTIONS {
            return false;
        }

        let mut ssl_config = Self::instance()
            .lock()
            .unwrap()
            .connections
            .remove(&id)
            .and_then(|connection| connection.ssl_config);

        let kind = match conn_type {
            "tcp" => ConnectionKind::Tcp,
            "udp" => ConnectionKind::Udp,
            "ssl" => ConnectionKind::Ssl,
            _ => return false,
        };

        let stream = match kind {
            ConnectionKind::Tcp | ConnectionKind::Ssl => {
                let remote = format!("{}:{}", addr, port);
                match TcpStream::connect(&remote).await {
                    Ok(s) => {
                        if keepalive > 0 {
                            let _ = s.set_nodelay(true);
                        }

                        if kind == ConnectionKind::Ssl {
                            match ssl_config.as_ref() {
                                Some(config) => match Self::upgrade_tcp_to_tls(s, addr, config).await {
                                    Ok(stream) => Some(stream),
                                    Err(error) => {
                                        error!("CIP start TLS connect error: {}", error);
                                        return false;
                                    }
                                },
                                None => Some(ConnStream::Tcp(s)),
                            }
                        } else {
                            ssl_config = None;
                            Some(ConnStream::Tcp(s))
                        }
                    }
                    Err(error) => {
                        error!("CIP start TCP connect error: {}", error);
                        return false;
                    }
                }
            }
            ConnectionKind::Udp => {
                ssl_config = None;
                match UdpSocket::bind("0.0.0.0:0").await {
                    Ok(s) => {
                        let remote = format!("{}:{}", addr, port);
                        if let Err(error) = s.connect(&remote).await {
                            error!("CIP start UDP connect error: {}", error);
                            return false;
                        }
                        Some(ConnStream::Udp(s))
                    }
                    Err(error) => {
                        error!("CIP start UDP bind error: {}", error);
                        return false;
                    }
                }
            }
        };

        Self::instance().lock().unwrap().connections.insert(
            id,
            Connection {
                kind,
                addr: addr.to_string(),
                ssl_config,
                stream,
            },
        );
        true
    }

    pub fn close_connection(id: usize) -> bool {
        Self::instance().lock().unwrap().connections.remove(&id).is_some()
    }

    pub fn has_connection(id: usize) -> bool {
        Self::instance().lock().unwrap().connections.contains_key(&id)
    }

    pub async fn send_data(id: usize, length: usize) -> (bool, i32) {
        let mut session = DataModeSession::new(std::time::Duration::from_secs(30), Some(0x1a));
        let body = session.read_to_length(length).await;
        if body.finish != DataModeFinish::Complete {
            return (true, -1);
        }

        let Some(mut connection) = Self::instance().lock().unwrap().connections.remove(&id) else {
            return (true, 0);
        };

        let sent = match &mut connection.stream {
                Some(ConnStream::Tcp(stream)) => match stream.write_all(&body.data).await {
                    Ok(_) => body.data.len() as i32,
                    Err(error) => {
                        error!("CIP send TCP error: {}", error);
                        0
                    }
                },
                Some(ConnStream::Tls(stream)) => match stream.write_all(&body.data).await {
                    Ok(_) => body.data.len() as i32,
                    Err(error) => {
                        error!("CIP send TLS error: {}", error);
                        0
                    }
                },
                Some(ConnStream::Udp(stream)) => match stream.send(&body.data).await {
                    Ok(n) => n as i32,
                    Err(error) => {
                        error!("CIP send UDP error: {}", error);
                        0
                    }
                },
                None => 0,
            };

        Self::instance().lock().unwrap().connections.insert(id, connection);
        (true, sent)
    }

    pub async fn configure_ssl(
        id: usize,
        auth_mode: i32,
        check_hostname: Option<bool>,
        key: &str,
        cert: &str,
        ca: &str,
    ) -> Result<()> {
        if id >= MAX_CONNECTIONS {
            anyhow::bail!("Invalid connection id: {}", id);
        }

        let ssl_config = AtSslConfig::new(auth_mode, check_hostname, key, cert, ca)?;
        ssl_config.build_openssl_connector()?;

        let mut connection = Self::instance().lock().unwrap().connections.remove(&id).unwrap_or_else(|| Connection {
            kind: ConnectionKind::Ssl,
            addr: String::new(),
            ssl_config: None,
            stream: None,
        });

        connection.ssl_config = Some(ssl_config.clone());

        if connection.kind != ConnectionKind::Ssl {
            Self::instance().lock().unwrap().connections.insert(id, connection);
            return Ok(());
        }

        if connection.addr.is_empty() {
            Self::instance().lock().unwrap().connections.insert(id, connection);
            return Ok(());
        }

        let stream = connection.stream.take();
        connection.stream = match stream {
            Some(ConnStream::Tcp(stream)) => match Self::upgrade_tcp_to_tls(stream, &connection.addr, &ssl_config).await {
                Ok(stream) => Some(stream),
                Err(error) => {
                    connection.ssl_config = None;
                    Self::instance().lock().unwrap().connections.insert(id, connection);
                    error!("CIP configure SSL handshake error: {}", error);
                    return Err(error);
                }
            },
            Some(other) => Some(other),
            None => None,
        };

        Self::instance().lock().unwrap().connections.insert(id, connection);
        Ok(())
    }

    async fn upgrade_tcp_to_tls(
        stream: TcpStream,
        addr: &str,
        config: &AtSslConfig,
    ) -> Result<ConnStream> {
        let connector = config.build_openssl_connector()?;
        let mut ssl_config = connector.configure()?;
        ssl_config.set_verify_hostname(config.check_hostname());

        let ssl = ssl_config.into_ssl(addr)?;
        let mut stream = tokio_openssl::SslStream::new(ssl, stream)?;
        Pin::new(&mut stream).connect().await?;
        Ok(ConnStream::Tls(Box::new(stream)))
    }
}
