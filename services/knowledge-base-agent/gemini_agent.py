from langchain_core.tools import StructuredTool
from langchain_google_genai import ChatGoogleGenerativeAI
from langchain.agents import create_agent
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import BaseModel
import asyncio
import os
from dotenv import load_dotenv

load_dotenv()

# 1. Persistent MCP Session Manager
class MCPSessionManager:
    def __init__(self):
        self.session = None
        self.exit_stack = None

    async def start(self):
        print("DEBUG: Starting MCP server process...")
        server_params = StdioServerParameters(
            command="cargo",
            args=["run", "--bin", "mcp-server"],
            cwd="/home/nithesh/coding/Coding/Project/Coding/Project/KnowledgeSearch",
            env={**os.environ}
        )
        
        from contextlib import AsyncExitStack
        self.exit_stack = AsyncExitStack()
        
        print("DEBUG: Connecting via stdio...")
        read, write = await self.exit_stack.enter_async_context(stdio_client(server_params))
        print("DEBUG: Creating session...")
        self.session = await self.exit_stack.enter_async_context(ClientSession(read, write))
        print("DEBUG: Initializing session...")
        await self.session.initialize()
        print("DEBUG: Session initialized successfully.")

    async def get_tools(self):
        mcp_tools = await self.session.list_tools()
        langchain_tools = []
        
        for t in mcp_tools.tools:
            print(f"DEBUG: Mapping MCP tool: {t.name}")
            
            async def mcp_tool_wrapper(name=t.name, **kwargs):
                print(f"DEBUG: Tool {name} called with args: {kwargs}")
                result = await self.session.call_tool(name, arguments=kwargs)
                return result.content[0].text
            
            def mcp_tool_sync_wrapper(name=t.name, **kwargs):
                return asyncio.run(mcp_tool_wrapper(name=name, **kwargs))
            
            # Properly construct a Pydantic model from the JSON schema
            from pydantic import create_model
            
            # Simplified schema mapping for prototype
            # In production, we would map the full JSON schema to Pydantic fields
            fields = {
                name: (str, ...) for name in t.inputSchema.get('required', [])
            }
            dynamic_schema = create_model(f"{t.name}Schema", **fields)
            
            langchain_tools.append(StructuredTool.from_function(
                func=mcp_tool_sync_wrapper,
                coroutine=mcp_tool_wrapper,
                name=t.name, 
                description=t.description,
                args_schema=dynamic_schema
            ))
            
        return langchain_tools

    async def close(self):
        if self.exit_stack:
            await self.exit_stack.aclose()

# 2. Setup Gemini Agent
async def run_agent(query: str):
    manager = MCPSessionManager()
    await manager.start()
    
    try:
        tools = await manager.get_tools()
        llm = ChatGoogleGenerativeAI(
            model="gemini-2.5-flash-lite",
            api_key=os.environ.get("GEMINI_API_KEY"),
            max_retries=3
        )

        
        # New agent creation syntax
        agent = create_agent(
            model=llm,
            tools=tools,
            system_prompt="You are a helpful knowledge base agent. Use your tools to answer user queries."
        )
        
        # Run agent
        response = agent.invoke({"messages": [("user", query)]})
        print(response["messages"][-1].content)
        
    finally:
        await manager.close()

if __name__ == "__main__":
    asyncio.run(run_agent("Search for information about os3"))
