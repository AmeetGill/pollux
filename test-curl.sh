curl http://localhost:1234/chat \
        -H"Host: server.cluster23.com" \
        -H"Upgrade: websocket" \
        -H"Connection: Upgrade" \
        -H"Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
        -H"Sec-WebSocket-Protocol: chat, v1.chat.cluster23.com" \
        -H"Sec-WebSocket-Version: 13" \
        -H"Origin: cluster23.com" -v
