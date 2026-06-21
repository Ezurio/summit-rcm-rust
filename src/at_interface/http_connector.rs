//
// SPDX-License-Identifier: LicenseRef-Ezurio-Clause
// Copyright (C) 2026 Ezurio LLC.
//

//! OpenSSL-backed ureq connector for the AT-interface HTTP transaction service.
//!
//! ureq drives the HTTP protocol (redirects, chunked transfer-encoding, response
//! parsing). ureq ships rustls/native-tls connectors but not OpenSSL, so this
//! module provides the OpenSSL bridge — mirroring ureq's own TLS connectors — so
//! the AT interface keeps its OpenSSL mutual-auth and selective certificate
//! verification behaviour.

use crate::at_interface::ssl::AtSslConfig;
use std::fmt;
use std::io::{Read, Write};

use openssl::ssl::SslStream;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, Either, LazyBuffers, NextTimeout, Transport,
    TransportAdapter,
};

/// ureq connector that wraps a chained TCP transport in an OpenSSL TLS stream
/// using the AT interface's [`AtSslConfig`]. Mirrors ureq's own TLS connectors
/// so that redirects, chunked decoding and connection handling stay in ureq.
#[derive(Debug)]
pub(crate) struct AtOpenSslConnector {
    pub(crate) ssl_config: Option<AtSslConfig>,
}

impl<In: Transport> Connector<In> for AtOpenSslConnector {
    type Out = Either<In, AtOpenSslTransport>;

    fn connect(
        &self,
        details: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        let Some(transport) = chained else {
            return Ok(None);
        };

        // Pass the plain transport through for non-TLS targets or if a previous
        // connector already established TLS.
        if !details.needs_tls() || transport.is_tls() {
            return Ok(Some(Either::A(transport)));
        }

        let Some(ssl_config) = &self.ssl_config else {
            return Ok(Some(Either::A(transport)));
        };

        let host = details
            .uri
            .authority()
            .expect("uri authority for tls")
            .host()
            .to_string();

        let connector = ssl_config
            .build_openssl_connector()
            .map_err(|e| ureq::Error::Io(std::io::Error::other(e)))?;
        let ssl = connector
            .configure()
            .map_err(|e| ureq::Error::Io(std::io::Error::other(e)))?
            .into_ssl(&host)
            .map_err(|e| ureq::Error::Io(std::io::Error::other(e)))?;
        let mut stream = SslStream::new(ssl, TransportAdapter::new(transport.boxed()))
            .map_err(|e| ureq::Error::Io(std::io::Error::other(e)))?;
        stream
            .connect()
            .map_err(|e| ureq::Error::Io(std::io::Error::other(e)))?;

        let buffers = LazyBuffers::new(
            details.config.input_buffer_size(),
            details.config.output_buffer_size(),
        );

        Ok(Some(Either::B(AtOpenSslTransport { buffers, stream })))
    }
}

/// TLS transport produced by [`AtOpenSslConnector`].
pub(crate) struct AtOpenSslTransport {
    buffers: LazyBuffers,
    stream: SslStream<TransportAdapter>,
}

impl Transport for AtOpenSslTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let output = &self.buffers.output()[..amount];
        self.stream.write_all(output)?;
        Ok(())
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let input = self.buffers.input_append_buf();
        let amount = self.stream.read(input)?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }

    fn is_open(&mut self) -> bool {
        self.stream.get_mut().get_mut().is_open()
    }

    fn is_tls(&self) -> bool {
        true
    }
}

impl fmt::Debug for AtOpenSslTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AtOpenSslTransport").finish()
    }
}
