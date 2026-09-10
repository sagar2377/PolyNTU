FROM node:22-bookworm-slim AS frontend
WORKDIR /build/frontend
COPY frontend/package*.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM rust:1.88-bookworm AS backend
WORKDIR /build/backend
COPY backend/ ./
RUN cargo build --release --locked --bin polyntu

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /build/backend/target/release/polyntu /app/polyntu
COPY --from=frontend /build/frontend/dist /app/frontend
ENV POLYNTU_FRONTEND_DIR=/app/frontend POLYNTU_BIND=0.0.0.0:8000
USER 10001:10001
EXPOSE 8000
CMD ["/app/polyntu"]
