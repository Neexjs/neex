import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  transpilePackages: ["@{{projectName}}/ui", "@{{projectName}}/utils"],
};

export default nextConfig;
