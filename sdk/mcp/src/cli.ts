#!/usr/bin/env node
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { IdbSeqClient } from 'idbseq';
import { ConfigError, parseCommand, USAGE } from './config.js';
import { createIdbSeqServer } from './server.js';
import { StartupError, verifyConnection } from './startup.js';

// stdout carries the MCP protocol; everything for people goes to stderr.
function report(message: string): void {
  process.stderr.write(`idbseq-mcp: ${message}\n`);
}

async function main(): Promise<void> {
  const command = parseCommand(process.argv.slice(2), process.env);
  if (command.kind === 'help') {
    process.stderr.write(`${USAGE}\n`);
    return;
  }
  const { config } = command;
  config.warnings.forEach((warning) => report(`warning: ${warning}`));
  const client = new IdbSeqClient({ url: config.url, apiKey: config.apiKey });
  await verifyConnection(client, config.url);
  const server = createIdbSeqServer(client, { allowCreate: config.allowCreate });
  await server.connect(new StdioServerTransport());
  report(`serving ${config.url}${config.allowCreate ? ' (create_collection enabled)' : ''}`);
}

main().catch((error: unknown) => {
  const isExpected = error instanceof ConfigError || error instanceof StartupError;
  report(isExpected ? error.message : `unexpected error: ${error instanceof Error ? error.stack : String(error)}`);
  process.exitCode = 1;
});
