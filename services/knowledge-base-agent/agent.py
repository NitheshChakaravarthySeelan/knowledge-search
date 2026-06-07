from langgraph.graph import StateGraph, END
from typing import Annotated, TypedDict, List
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client
import json
import asyncio
import os

# 1. Define State
class AgentState(TypedDict):
    messages: List[str]  # Simplified for this prototype
    tool_results: List[str]

# 2. Define MCP Tool Interface
async def call_mcp_tool(tool_name: str, args: dict):
    # This assumes the mcp server is running as a local command
    server_params = StdioServerParameters(
        command="cargo",
        args=["run", "--bin", "mcp-server"],
        cwd="/home/nithesh/coding/Coding/Project/Coding/Project/KnowledgeSearch",
        env={**os.environ}
    )

    async with stdio_client(server_params) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()
            result = await session.call_tool(tool_name, arguments=args)
            return result.content[0].text

# 3. Define Nodes
async def agent_node(state: AgentState):
    # In a full LangGraph, you'd have an LLM here.
    # For this prototype, we simulate the agent deciding to call tools.
    last_message = state["messages"][-1]
    
    if "ingest" in last_message.lower():
        # Simulated ingestion logic
        path = "/home/nithesh/Downloads/os3pieces.pdf"
        result = await call_mcp_tool("ingest_pdf", {"path": path})
        return {"tool_results": [f"Ingestion result: {result}"]}
    elif "search" in last_message.lower():
        # Simulated search logic
        query = "what is os3?"
        tenant = "default"
        result = await call_mcp_tool("search_knowledge_base", {"query": query, "tenant_id": tenant})
        return {"tool_results": [f"Search result: {result}"]}
    
    return {"tool_results": ["No tool called"]}

# 4. Build Graph
workflow = StateGraph(AgentState)
workflow.add_node("agent", agent_node)
workflow.set_entry_point("agent")
workflow.add_edge("agent", END)

app = workflow.compile()

# Example execution
if __name__ == "__main__":
    initial_state = {"messages": ["Please search for information about os3"], "tool_results": []}
    final_state = asyncio.run(app.ainvoke(initial_state))
    print(final_state)
