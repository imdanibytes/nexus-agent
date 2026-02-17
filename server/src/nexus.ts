import { NexusServer } from "@imdanibytes/nexus-sdk/server";

// NEXUS_HOST_URL is the Docker-reachable host address (host.docker.internal:9600).
// The SDK defaults to NEXUS_API_URL which is localhost:9600 — meant for the browser
// SDK, unreachable from inside Docker. Pass apiUrl explicitly until the SDK fix ships.
export const nexus = new NexusServer({
  apiUrl: process.env.NEXUS_HOST_URL,
});
