# ShadowBoy Netplay TypeScript SDK

Pure TypeScript relay-contract SDK for the ShadowBoy Electron client.

The SDK owns protocol shapes and client-side relay state. Electron supplies
transport adapters, protected request signing, emulator integration, and UI.

```bash
cd /home/catalyst-2/projects/sb-desktop/sb-netplay-serv
bun test sdk/typescript/tests/**/*.test.ts
bunx tsc --noEmit -p sdk/typescript/tsconfig.json
```
