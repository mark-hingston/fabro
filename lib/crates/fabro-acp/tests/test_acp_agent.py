#!/usr/bin/env python3
"""Minimal ACP agent for integration testing over stdio.

Speaks JSON-RPC 2.0 over stdin/stdout per the ACP specification.
Handles: initialize, session/new, session/prompt, session/cancel.
"""

import json
import sys
import uuid

PROTOCOL_VERSION = 1

AGENT_INFO = {
    "name": "test-acp-agent",
    "version": "0.1.0",
}

AGENT_CAPABILITIES = {
    "promptCapabilities": {
        "image": False,
        "audio": False,
        "embeddedContext": False,
    },
    "sessionCapabilities": {
        "list": {},
    },
}

sessions = {}


def handle_request(req):
    method = req.get("method")
    req_id = req.get("id")
    params = req.get("params", {})

    if method == "initialize":
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "agentCapabilities": AGENT_CAPABILITIES,
                "authMethods": [],
                "agentInfo": AGENT_INFO,
            },
        }

    if method == "session/new":
        session_id = str(uuid.uuid4())
        sessions[session_id] = {"cwd": params.get("cwd", "/tmp")}
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "sessionId": session_id,
            },
        }

    if method == "session/prompt":
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "stopReason": "end_turn",
            },
        }

    if method == "session/cancel":
        return None

    if req_id is None:
        return None

    return {
        "jsonrpc": "2.0",
        "id": req_id,
        "error": {"code": -32601, "message": f"Method not found: {method}"},
    }


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue

        resp = handle_request(req)
        if resp is not None:
            sys.stdout.write(json.dumps(resp) + "\n")
            sys.stdout.flush()


if __name__ == "__main__":
    main()
