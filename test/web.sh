#!/bin/sh

echo "Testing web server..."

echo "\nGET /hello"
curl http://localhost:9759/hello

echo "\nPOST /cmd"
curl -X POST http://localhost:9759/cmd -H "Content-Type: application/json" -d '{"cmd": "p plugins show"}'
