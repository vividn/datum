import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  webpack: (config, { isServer }) => {
    if (!isServer) {
      // Don't attempt to load native modules on the server
      config.resolve.fallback = {
        ...config.resolve.fallback,
        "pouchdb": require.resolve("pouchdb"),
        "fs": false,
        "path": false
      };
    }
    return config;
  },
};

export default nextConfig;
