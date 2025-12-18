FROM alpine:3.18
RUN apk add --no-cache protobuf nodejs npm
RUN npm install -g protoc-gen-js ts-proto@1.181.0
RUN wget https://github.com/grpc/grpc-web/releases/download/1.5.0/protoc-gen-grpc-web-1.5.0-linux-x86_64 -O /usr/local/bin/protoc-gen-grpc-web && \
    chmod +x /usr/local/bin/protoc-gen-grpc-web
WORKDIR /workspace
ENTRYPOINT ["protoc"]
