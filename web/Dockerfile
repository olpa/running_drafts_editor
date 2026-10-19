# syntax=docker/dockerfile:1
FROM node:24.21.0-alpine3.24 AS builder
WORKDIR /app
COPY package.json package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci
COPY . .
RUN npm run build

FROM nginx:1.31.1-alpine3.23 AS runtime
COPY deployment/nginx.conf /etc/nginx/nginx.conf
COPY deployment/container-entrypoint.sh /usr/local/bin/running-drafts-frontend
COPY --from=builder /app/dist /usr/share/nginx/html
RUN chmod 0755 /usr/local/bin/running-drafts-frontend
USER nginx
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/running-drafts-frontend"]
