#!/bin/sh

set -eux

TS_PROTO="$(pnpm bin)/protoc-gen-ts_proto"

FLAGS="--experimental_allow_proto3_optional"
TS_FLAGS="--plugin=${TS_PROTO} \
            --ts_proto_opt=esModuleInterop=true \
            --ts_proto_opt=oneof=unions \
            --ts_proto_opt=fileSuffix=.pb \
            --ts_proto_opt=importSuffix=.js \
            --ts_proto_opt=removeEnumPrefix=true \
            --ts_proto_opt=useExactTypes=false \
            --ts_proto_opt=globalThisPolyfill=true"

API="./fig.proto ./fig_common.proto ./figterm.proto ./remote.proto ./local.proto ./mux.proto"

OUT="dist"

# clean the out dir
rm -rf "$OUT"
mkdir -p "$OUT"

# shellcheck disable=SC2086
protoc $FLAGS $TS_FLAGS "--ts_proto_out=$OUT" $API

tsc
