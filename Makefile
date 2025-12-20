.PHONY: build build-agent build-agent-windows build-server build-web generate-web-proto

build-all: build-agent build-agent-windows build-server

build-agent:
	mkdir -p build
	docker build -f agent/Dockerfile --target export --output type=local,dest=./build .

build-agent-windows:
	mkdir -p build
	docker build -f agent/Dockerfile.windows --target export --output type=local,dest=./build .

build-server: build-web
	mkdir -p build
	docker build -f server/Dockerfile --target export --output type=local,dest=./build .

build-web: generate-web-proto
	docker build -f web/build.Dockerfile -t mikaboshi-web-builder web --output type=local,dest=./build/web/new
	rm -rf build/web/dist
	mv build/web/new build/web/dist

generate-web-proto:
	docker build -f web/proto.Dockerfile -t mikaboshi-proto-generator web
	docker run --rm -v $(PWD):/workspace mikaboshi-proto-generator \
		-I=proto --plugin=protoc-gen-ts_proto=/usr/local/bin/protoc-gen-ts_proto \
		--ts_proto_out=web/src/proto \
		--ts_proto_opt=outputServices=default,env=browser,useObservables=true,esModuleInterop=true \
		packet.proto
	