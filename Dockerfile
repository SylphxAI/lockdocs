# Build from repository root so the catalogue and technical docs use their canonical sources.
FROM node:22.23.5-bookworm-slim AS dependencies
WORKDIR /workspace/apps/web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci --include=dev --no-audit --no-fund

FROM node:22.23.5-bookworm-slim AS build
WORKDIR /workspace/apps/web
COPY --from=dependencies /workspace/apps/web/node_modules ./node_modules
COPY apps/web/ ./
COPY config/money-catalogue.json /workspace/config/money-catalogue.json
COPY docs/ /workspace/docs/
RUN npm run build

# Keep the exact build-stage Node binary and Debian 12 runtime libraries.
# Nitro needs neither npm nor a shell in the deployed image.
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
ENV NODE_ENV=production HOST=0.0.0.0 NITRO_HOST=0.0.0.0 PORT=3000
COPY --from=build /usr/local/bin/node /usr/local/bin/node
COPY --from=build --chown=65532:65532 /workspace/apps/web/.output ./.output
USER 65532:65532
EXPOSE 3000
CMD ["/usr/local/bin/node", ".output/server/index.mjs"]
