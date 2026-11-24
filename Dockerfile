# Glama inspects this stdio MCP image. Sylphx Hosting uses Dockerfile.web.
FROM node:22.23.5-bookworm-slim

# Use a published release so inspection doesn't need a Rust build or a package
# release from the commit being inspected.
RUN npm install --global --no-audit --no-fund @sylphx/lockdocs@0.7.8 \
    && npm cache clean --force

USER node
CMD ["lockdocs", "mcp"]
