import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  allowedDevOrigins: ["192.168.6.44"],
  async rewrites() {
    const backend = process.env.BACKEND_URL ?? `http://localhost:${process.env.BACKEND_PORT ?? "3003"}`;
    return [
      {
        source: "/api/:path*",
        destination: `${backend}/api/:path*`,
      },
    ];
  },
};

export default nextConfig;
