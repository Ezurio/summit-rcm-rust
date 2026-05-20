#!/usr/bin/env bash

if [[ -n "${BASH_VERSION:-}" ]]; then
  shopt -s expand_aliases 2>/dev/null || true
fi

SUMMIT_API_HOST=${SUMMIT_API_HOST:-test.summit.com}
SUMMIT_API_CA=${SUMMIT_API_CA:-/devel/cp_linux/som-external/board/configs-common/keys/rest-server/ca.crt}
SUMMIT_API_USERNAME=${SUMMIT_API_USERNAME:-root}
SUMMIT_API_PASSWORD=${SUMMIT_API_PASSWORD:-summit}
SUMMIT_API_COOKIE_ROOT=${SUMMIT_API_COOKIE_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/summit-rcm-api}

RUST_TARGET_IP=${RUST_TARGET_IP:-10.10.4.164}
PY_TARGET_IP=${PY_TARGET_IP:-10.10.4.133}

mkdir -p "$SUMMIT_API_COOKIE_ROOT"

SUMMIT_API_CONNECT_RETRIES=${SUMMIT_API_CONNECT_RETRIES:-10}
SUMMIT_API_CONNECT_TIMEOUT=${SUMMIT_API_CONNECT_TIMEOUT:-20}

summit_curl_common_args() {
  local ip=${1:?target ip required}
  printf '%s\n' \
    "-ksS" \
    "--max-time" \
    "$SUMMIT_API_CONNECT_TIMEOUT" \
    "--retry" \
    "$SUMMIT_API_CONNECT_RETRIES" \
    "--retry-connrefused" \
    "--retry-all-errors" \
    "--cacert" \
    "$SUMMIT_API_CA" \
    "--resolve" \
    "$SUMMIT_API_HOST:443:$ip"
}

summit_http_status() {
  local headers=${1:?headers file required}
  sed -n '1s/.* \([0-9][0-9][0-9]\) .*/\1/p' "$headers"
}

summit_cookie_jar() {
  local name=${1:?cookie jar name required}
  printf '%s/%s.cookies.txt\n' "$SUMMIT_API_COOKIE_ROOT" "$name"
}

summit_login() {
  local name=${1:?target name required}
  local ip=${2:?target ip required}
  local jar
  jar=$(summit_cookie_jar "$name")

  rm -f "$jar"

  curl $(summit_curl_common_args "$ip") \
    -c "$jar" \
    -H 'Content-Type: application/json' \
    -d "{\"username\":\"$SUMMIT_API_USERNAME\",\"password\":\"$SUMMIT_API_PASSWORD\"}" \
    "https://$SUMMIT_API_HOST/api/v2/login"
}

summit_request() {
  local name=${1:?target name required}
  local ip=${2:?target ip required}
  local method=${3:?http method required}
  local path=${4:?api path required}
  local data=${5:-}
  local jar
  local headers
  local body
  local status
  jar=$(summit_cookie_jar "$name")
  headers=$(mktemp)
  body=$(mktemp)

  if [[ -n "$data" ]]; then
    curl $(summit_curl_common_args "$ip") \
      -b "$jar" \
      -c "$jar" \
      -X "$method" \
      -H 'Content-Type: application/json' \
      -d "$data" \
      -D "$headers" \
      -o "$body" \
      "https://$SUMMIT_API_HOST$path"
  else
    curl $(summit_curl_common_args "$ip") \
      -b "$jar" \
      -c "$jar" \
      -X "$method" \
      -D "$headers" \
      -o "$body" \
      "https://$SUMMIT_API_HOST$path"
  fi

  status=$(summit_http_status "$headers")
  if [[ "$status" == "401" || "$status" == "403" ]]; then
    summit_login "$name" "$ip" >/dev/null
    if [[ -n "$data" ]]; then
      curl $(summit_curl_common_args "$ip") \
        -b "$jar" \
        -c "$jar" \
        -X "$method" \
        -H 'Content-Type: application/json' \
        -d "$data" \
        -D "$headers" \
        -o "$body" \
        "https://$SUMMIT_API_HOST$path"
    else
      curl $(summit_curl_common_args "$ip") \
        -b "$jar" \
        -c "$jar" \
        -X "$method" \
        -D "$headers" \
        -o "$body" \
        "https://$SUMMIT_API_HOST$path"
    fi
  fi

  cat "$body"
  rm -f "$headers" "$body"
}

summit_status() {
  local name=${1:?target name required}
  local ip=${2:?target ip required}
  local method=${3:?http method required}
  local path=${4:?api path required}
  local data=${5:-}
  local jar
  local headers
  local body

  jar=$(summit_cookie_jar "$name")
  headers=$(mktemp)
  body=$(mktemp)

  if [[ -n "$data" ]]; then
    curl $(summit_curl_common_args "$ip") \
      -b "$jar" \
      -c "$jar" \
      -X "$method" \
      -H 'Content-Type: application/json' \
      -d "$data" \
      -D "$headers" \
      -o "$body" \
      "https://$SUMMIT_API_HOST$path"
  else
    curl $(summit_curl_common_args "$ip") \
      -b "$jar" \
      -c "$jar" \
      -X "$method" \
      -D "$headers" \
      -o "$body" \
      "https://$SUMMIT_API_HOST$path"
  fi

  local status
  status=$(summit_http_status "$headers")
  if [[ "$status" == "401" || "$status" == "403" ]]; then
    summit_login "$name" "$ip" >/dev/null
    if [[ -n "$data" ]]; then
      curl $(summit_curl_common_args "$ip") \
        -b "$jar" \
        -c "$jar" \
        -X "$method" \
        -H 'Content-Type: application/json' \
        -d "$data" \
        -D "$headers" \
        -o "$body" \
        "https://$SUMMIT_API_HOST$path"
    else
      curl $(summit_curl_common_args "$ip") \
        -b "$jar" \
        -c "$jar" \
        -X "$method" \
        -D "$headers" \
        -o "$body" \
        "https://$SUMMIT_API_HOST$path"
    fi
  fi

  sed -n '1p' "$headers"
  cat "$body"
  rm -f "$headers" "$body"
}

rust_login() {
  summit_login rust "$RUST_TARGET_IP"
}

py_login() {
  summit_login python "$PY_TARGET_IP"
}

rust_get() {
  summit_request rust "$RUST_TARGET_IP" GET "$1"
}

py_get() {
  summit_request python "$PY_TARGET_IP" GET "$1"
}

rust_status() {
  summit_status rust "$RUST_TARGET_IP" GET "$1"
}

py_status() {
  summit_status python "$PY_TARGET_IP" GET "$1"
}