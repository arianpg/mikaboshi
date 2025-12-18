import { grpc } from "@improbable-eng/grpc-web";
import { Observable } from "rxjs";

class ByteWrapper {
    constructor(public bytes: Uint8Array) { }
    serializeBinary(): Uint8Array {
        return this.bytes;
    }
    toObject(): any {
        return {};
    }
    static deserializeBinary(bytes: Uint8Array): ByteWrapper {
        return new ByteWrapper(bytes);
    }
}

export class GrpcWebImpl {
    private host: string;
    private options: {
        transport?: grpc.TransportFactory;
        debug?: boolean;
        metadata?: grpc.Metadata;
    };

    constructor(
        host: string,
        options: {
            transport?: grpc.TransportFactory;
            debug?: boolean;
            metadata?: grpc.Metadata;
        },
    ) {
        this.host = host;
        this.options = options;
    }

    unary(methodDesc: any, _request: any, metadata: any) {
        throw new Error("not implemented");
    }

    invoke(methodDesc: any, _request: any, metadata: any) {
        throw new Error("not implemented");
    }

    request(service: string, method: string, data: Uint8Array): Promise<Uint8Array> {
        return new Promise((resolve, reject) => {
            const methodDesc = {
                methodName: method,
                service: { serviceName: service },
                requestStream: false,
                responseStream: false,
                requestType: ByteWrapper,
                responseType: ByteWrapper,
            };

            const req = new ByteWrapper(data);

            grpc.unary(methodDesc as any, {
                request: req,
                host: this.host,
                metadata: this.options.metadata,
                transport: this.options.transport,
                debug: this.options.debug,
                onEnd: (res) => {
                    if (res.status === grpc.Code.OK) {
                        resolve((res.message as unknown as ByteWrapper).bytes);
                    } else {
                        reject(res);
                    }
                },
            });
        });
    }

    clientStreamingRequest(service: string, method: string, data: Observable<Uint8Array>): Promise<Uint8Array> {
        throw new Error("Client Streaming not supported in grpc-web");
    }

    serverStreamingRequest(service: string, method: string, data: Uint8Array): Observable<Uint8Array> {
        return new Observable((observer) => {
            const methodDesc = {
                methodName: method,
                service: { serviceName: service },
                requestStream: false,
                responseStream: true,
                requestType: ByteWrapper,
                responseType: ByteWrapper,
            };

            const req = new ByteWrapper(data);

            const client = grpc.invoke(methodDesc as any, {
                request: req,
                host: this.host,
                metadata: this.options.metadata,
                transport: this.options.transport,
                debug: this.options.debug,
                onMessage: (msg) => {
                    observer.next((msg as unknown as ByteWrapper).bytes);
                },
                onEnd: (code, msg, trailers) => {
                    if (code === grpc.Code.OK) {
                        observer.complete();
                    } else {
                        observer.error({ code, msg, trailers });
                    }
                },
            });

            return () => client.close();
        });
    }

    bidirectionalStreamingRequest(service: string, method: string, data: Observable<Uint8Array>): Observable<Uint8Array> {
        throw new Error("Bidirectional Streaming not supported in grpc-web");
    }
}
