#!/bin/sh

PRIVATE_KEY_FILE="private_key"

if [ -f "$PRIVATE_KEY_FILE" ]; then
    cat $PRIVATE_KEY_FILE | xargs -t -I {} flyctl secrets set AGE_PRIVATE_KEY={}
fi
flyctl deploy --remote-only