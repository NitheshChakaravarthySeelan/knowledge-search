from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from contextlib import AsyncExitStack
import os
import logging
import asyncio

logger = logging.getLogger(__name__)

class MCPSessionManager:
    def __init__(self):
        self.session = None
        self.exit_stack = None

    async def start(self):
        server_params = StdioServerParameters(
            command="cargo",
            args=["run", "--bin", "mcp-server"],
            cwd="/home/nithesh/coding/Coding/Project/Coding/Project/KnowledgeSearch",
            env={**os.environ}
        )
        
        self.exit_stack = AsyncExitStack()
        
        read, write = await self.exit_stack.enter_async_context(stdio_client(server_params))
        self.session = await self.exit_stack.enter_async_context(ClientSession(read, write))
        await self.session.initialize()

    async def get_tools(self):
        from langchain_core.tools import StructuredTool
        from pydantic import BaseModel, create_model

        mcp_tools = await self.session.list_tools()
        langchain_tools = []
        
        for t in mcp_tools.tools:
            logger.info(f"DEBUG: Mapping MCP tool: {t.name}")
            
            async def mcp_tool_wrapper(name=t.name, **kwargs):
                logger.info(f"DEBUG: Attempting to call Rust MCP tool: {name} with {kwargs}")
                result = await self.session.call_tool(name, arguments=kwargs)
                logger.info(f"DEBUG: MCP tool {name} returned successfully.")
                return result.content[0].text
            
            # Create a dynamic schema based on the inputSchema if it exists
            schema = t.inputSchema if t.inputSchema else {}
            fields = {
                name: (str, ...) for name in schema.get('required', [])
            }
            dynamic_schema = create_model(f"{t.name}Schema", **fields)
            
            # Pass coroutine directly to StructuredTool.from_function
            langchain_tools.append(StructuredTool.from_function(
                coroutine=mcp_tool_wrapper,
                name=t.name, 
                description=t.description,
                args_schema=dynamic_schema
            ))
            
        return langchain_tools

    async def close(self):
        if self.exit_stack:
            await self.exit_stack.aclose()
