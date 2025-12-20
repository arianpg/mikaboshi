FROM alpine:latest
WORKDIR /app

# Copy server binary
COPY mikaboshi-server /app/mikaboshi-server

# Copy web assets
COPY web/dist /app/web/dist

# Expose ports
EXPOSE 8080 50051

# Run the server
ENTRYPOINT ["/app/mikaboshi-server"]
