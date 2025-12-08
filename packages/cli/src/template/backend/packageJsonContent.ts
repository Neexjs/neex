// Use the current neex version
const neexVersion = '0.7.4';

export default (projectName: string, includePrisma: boolean = true) => `{
  "name": "${projectName}",
  "version": "0.0.0",
  "private": true,
  "scripts": {
    "dev": "neex dev src/server.ts",
    "build": "tsc",
    "start": "node dist/server.js"
  },
  "dependencies": {
    "express": "^5.1.0",
    "neex": "^${neexVersion}",
    "cors": "^2.8.5",
    "dotenv": "^17.2.0",
    "chalk": "^5.6.2"${includePrisma ? ',\n    "@prisma/client": "^6.11.1"' : ''}
  },
  "devDependencies": {
    "typescript": "^5.8.3",
    "@types/express": "^5.0.3",
    "@types/cors": "^2.8.19",
    "@types/node": "^22.15.17",
    "@repo/typescript-config": "*",
    "@repo/biome-config": "*"${includePrisma ? ',\n    "prisma": "^6.11.1"' : ''}
  }
}`;
