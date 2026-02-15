const NEXUS_HOST_URL = process.env.NEXUS_HOST_URL || "http://host.docker.internal:9600";
const NEXUS_PLUGIN_SECRET = process.env.NEXUS_PLUGIN_SECRET || "";

let cachedAccessToken: string | null = null;
let tokenExpiresAt = 0;

export async function getAccessToken(): Promise<string> {
  if (cachedAccessToken && Date.now() < tokenExpiresAt - 30000) {
    return cachedAccessToken;
  }

  const res = await fetch(`${NEXUS_HOST_URL}/api/v1/auth/token`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ secret: NEXUS_PLUGIN_SECRET }),
  });

  if (!res.ok) {
    throw new Error(`Token exchange failed: ${res.status}`);
  }

  const data = await res.json() as { access_token: string; expires_in: number };
  cachedAccessToken = data.access_token;
  tokenExpiresAt = Date.now() + data.expires_in * 1000;
  return cachedAccessToken;
}
