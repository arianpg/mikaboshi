FROM alpine:latest
RUN apk add --no-cache protobuf nodejs npm
RUN npm install -g protoc-gen-js ts-proto grpc-web
WORKDIR /workspace
ENTRYPOINT ["protoc"]
