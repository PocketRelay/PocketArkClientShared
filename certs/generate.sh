#!/bin/bash

set -e

# -------------------------
# Variables
# -------------------------

CA_NAME="GOS 2015 Certificate Authority"
CA_KEY="gos2015-ca.key"
CA_CRT="gos2015-ca.crt"

SERVER_CN="winter15.gosredirector.ea.com"
SERVER_KEY="winter15.key"
SERVER_CSR="winter15.csr"
SERVER_CRT="winter15.crt"

OPENSSL_CONF="winter15-openssl.cnf"

# -------------------------
# Create OpenSSL Config for SANs
# -------------------------

cat > "$OPENSSL_CONF" <<EOF
[ req ]
default_bits       = 4096
prompt             = no
default_md         = sha256
distinguished_name = req_distinguished_name
req_extensions     = req_ext

[ req_distinguished_name ]
CN = winter15.gosredirector.ea.com
O  = Electronic Arts, Inc. Ltd
ST = California
C  = US

[ req_ext ]
subjectAltName = @alt_names

[ alt_names ]
DNS.1 = winter15.gosredirector.ea.com
DNS.2 = spring18.gosredirector.ea.com
EOF

echo "[+] OpenSSL SAN configuration created in $OPENSSL_CONF"

# -------------------------
# Create CA
# -------------------------

echo "[+] Generating CA private key..."
openssl genrsa -out "$CA_KEY" 4096

echo "[+] Generating CA certificate..."
MSYS2_ARG_CONV_EXCL='*' openssl req -x509 -new -nodes \
  -key "$CA_KEY" \
  -sha256 -days 3650 \
  -subj "/CN=GOS 2015 Certificate Authority/O=Electronic Arts, Inc. Ltd/ST=California/C=US" \
  -out "$CA_CRT"

# -------------------------
# Create server key + CSR
# -------------------------

echo "[+] Generating server private key..."
openssl genrsa -out "$SERVER_KEY" 4096

echo "[+] Generating server CSR..."
openssl req -new \
  -key "$SERVER_KEY" \
  -out "$SERVER_CSR" \
  -config "$OPENSSL_CONF"

# -------------------------
# Sign certificate with the CA
# -------------------------

echo "[+] Signing server certificate with CA..."
openssl x509 -req \
  -in "$SERVER_CSR" \
  -CA "$CA_CRT" \
  -CAkey "$CA_KEY" \
  -CAcreateserial \
  -out "$SERVER_CRT" \
  -days 3650 \
  -sha256 \
  -extensions req_ext \
  -extfile "$OPENSSL_CONF"

echo ""
echo "============================================"
echo "   Certificate Generation Complete"
echo "============================================"
echo "CA Key:      $CA_KEY"
echo "CA Cert:     $CA_CRT"
echo "Server Key:  $SERVER_KEY"
echo "Server CSR:  $SERVER_CSR"
echo "Server Cert: $SERVER_CRT"
echo ""
