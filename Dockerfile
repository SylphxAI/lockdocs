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

FROM node:22.23.5-bookworm-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production HOST=0.0.0.0 PORT=3000
COPY --from=build --chown=node:node /workspace/apps/web/.output ./.output
USER node
EXPOSE 3000
CMD ["node", ".output/server/index.mjs"]
