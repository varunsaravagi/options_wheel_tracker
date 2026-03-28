import type { NextConfig } from "next";
import { readFileSync } from "fs";
import { resolve } from "path";

// Load BACKEND_PORT from project root .env if not already in environment
if (!process.env.BACKEND_PORT) {
  try {
    const envFile = readFileSync(resolve(__dirname, "../.env"), "utf-8");
    const match = envFile.match(/^BACKEND_PORT=(\d+)/m);
    if (match) process.env.BACKEND_PORT = match[1];
  } catch {}
}

const nextConfig: NextConfig = {
  allowedDevOrigins: ["192.168.6.44"],
  async rewrites() {
    const backendPort = process.env.BACKEND_PORT ?? "3003";
    const backend = process.env.BACKEND_URL ?? `http://localhost:${backendPort}`;
    return [
      {
        source: "/api/:path*",
        destination: `${backend}/api/:path*`,
      },
    ];
  },
};

export default nextConfig;
