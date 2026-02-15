# Stage 1: Install all dependencies (workspace-level)
FROM node:20-alpine AS deps
WORKDIR /app
RUN corepack enable
COPY package.json pnpm-workspace.yaml .npmrc ./
COPY server/package.json ./server/
COPY ui/package.json ./ui/
RUN pnpm install --frozen-lockfile || pnpm install

# Stage 2: Build server + frontend
FROM deps AS build
COPY tsconfig.base.json ./
COPY server/ ./server/
COPY ui/ ./ui/
RUN pnpm -r build
RUN pnpm --filter server deploy --legacy /app/deployed

# Stage 3: Runtime
FROM node:20-alpine
WORKDIR /app
COPY --from=build /app/deployed/dist/ ./dist/
COPY --from=build /app/deployed/node_modules/ ./node_modules/
COPY --from=build /app/deployed/package.json ./
COPY --from=build /app/ui/dist/ ./public/
RUN mkdir -p /data/conversations /data/profiles
EXPOSE 80
VOLUME ["/data"]
CMD ["node", "dist/index.js"]
