FROM alpine:latest
RUN apk add --no-cache libpcap
WORKDIR /app

# Copy agent binary
COPY mikaboshi-agent /app/mikaboshi-agent

# Run the agent
ENTRYPOINT ["/app/mikaboshi-agent"]
