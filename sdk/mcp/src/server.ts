import { createRequire } from 'node:module';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { registerTools, RULES, type IdbSeqApi, type ToolOptions } from './tools.js';

export const SERVER_NAME = 'idbseq';

/** The package version, read once so it cannot drift from package.json. */
export const SERVER_VERSION: string = (createRequire(import.meta.url)('../package.json') as { version: string }).version;

/** Builds the MCP server with the idbseq tools; connect it to a transport to serve. */
export function createIdbSeqServer(api: IdbSeqApi, options: ToolOptions): McpServer {
  const server = new McpServer({ name: SERVER_NAME, version: SERVER_VERSION }, { instructions: RULES });
  registerTools(server, api, options);
  return server;
}
