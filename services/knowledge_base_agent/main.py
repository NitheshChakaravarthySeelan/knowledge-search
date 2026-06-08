from fastapi import FastAPI, Request
from contextlib import asynccontextmanager
from services.knowledge_base_agent.mcp_manager import MCPSessionManager
from langchain_google_genai import ChatGoogleGenerativeAI
from langchain.agents import create_agent
from langchain_core.callbacks import StdOutCallbackHandler
from pydantic import BaseModel
import os
import logging
from dotenv import load_dotenv

load_dotenv()

# Setup Logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class AskRequest(BaseModel):
    query: str

@asynccontextmanager
async def lifespan(app: FastAPI):
    logger.info("Initializing MCP Manager and Agent...")
    # Init MCP Manager
    manager = MCPSessionManager()
    await manager.start()
    
    # Init Agent
    tools = await manager.get_tools()
    llm = ChatGoogleGenerativeAI(
        model="gemini-2.5-flash-lite",
        api_key=os.environ.get("GEMINI_API_KEY"),
        max_retries=3
    )
    
    app.state.agent = create_agent(
        model=llm,
        tools=tools,
        system_prompt=(
            "You are an expert research assistant. "
            "Your primary goal is to answer questions using ONLY the information provided in the knowledge base. "
            "You MUST ALWAYS search the knowledge base using the 'search_knowledge_base' tool before answering. "
            "If the information is not found in the knowledge base, state that you cannot answer the question. "
            "Do not rely on your general knowledge."
        )
    )
    app.state.manager = manager
    logger.info("Agent and MCP Manager initialized.")
    yield
    await manager.close()
    logger.info("Agent shut down.")

app = FastAPI(lifespan=lifespan)

@app.post("/ask")
async def ask(payload: AskRequest, request: Request):
    logger.info(f"Received query: {payload.query}")
    try:
        # Perform agent invocation with callback for tracing
        handler = StdOutCallbackHandler()
        # Use ainvoke for true async tool execution
        response = await request.app.state.agent.ainvoke(
            {"messages": [("user", payload.query)]},
            {"callbacks": [handler]}
        )
        logger.info("Agent invocation successful.")
        
        # Return the final message content
        return {"answer": response["messages"][-1].content}
    except Exception as e:
        logger.error(f"Agent invocation failed: {e}", exc_info=True)
        return {"answer": "Agent Error", "error": str(e)}
