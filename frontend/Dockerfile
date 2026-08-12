# ── Build stage ───────────────────────────────────────────────────────────────
FROM node:20-alpine AS builder
WORKDIR /app

COPY package*.json ./
RUN npm ci

COPY . .
RUN npm run build

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM nginx:alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY security-headers.conf /etc/nginx/security-headers.conf
COPY cloudflare-origin-pull-ca.pem /etc/nginx/cloudflare-origin-pull-ca.pem
COPY --from=builder /app/dist /usr/share/nginx/html
EXPOSE 80
