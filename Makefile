.PHONY: build build-agent build-agent-windows build-server build-web generate-web-proto build-docker-server

build-all: build-agent build-agent-windows build-server

build-docker-server: build-server
	docker build -f server/runtime.Dockerfile -t mikaboshi-server:latest build/server

build-agent:
	mkdir -p build/agent
	docker build -f agent/Dockerfile --target export --output type=local,dest=./build/agent .

build-agent-windows:
	mkdir -p build/agent
	docker build -f agent/Dockerfile.windows --target export --output type=local,dest=./build/agent .

build-server: build-web
	mkdir -p build/server
	docker build -f server/Dockerfile --target export --output type=local,dest=./build/server .

build-web: generate-web-proto
	mkdir -p build/server/web
	docker build -f web/build.Dockerfile -t mikaboshi-web-builder web --output type=local,dest=./build/server/web/new
	rm -rf build/server/web/dist
	mv build/server/web/new build/server/web/dist

generate-web-proto:
	docker build -f web/proto.Dockerfile -t mikaboshi-proto-generator web
	docker run --rm -v $(PWD):/workspace mikaboshi-proto-generator \
		-I=proto --plugin=protoc-gen-ts_proto=/usr/local/bin/protoc-gen-ts_proto \
		--ts_proto_out=web/src/proto \
		--ts_proto_opt=outputServices=default,env=browser,useObservables=true,esModuleInterop=true,outputClientImpl=grpc-web \
		packet.proto
	