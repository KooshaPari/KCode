#!/bin/bash
export PATH="$HOME/.cargo/bin:$PATH"
cd /Users/kooshapari/CodeProjects/kcode
export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.sdk
export MACOSX_DEPLOYMENT_TARGET=15.0

RESULT_FILE="/tmp/kcode-test-results.txt"
echo "STARTED $(date)" > "$RESULT_FILE"
echo "Waiting for cargo lock contention to clear..." >> "$RESULT_FILE"

ATTEMPTS=0
while true; do
    ATTEMPTS=$((ATTEMPTS + 1))
    echo "$(date +%H:%M:%S) Attempt $ATTEMPTS: Running cargo test..." >> "$RESULT_FILE"

    cargo test --lib --bins > /tmp/kcode-test-output.txt 2>&1
    EXIT_CODE=$?

    if grep -q "Blocking waiting for file lock" /tmp/kcode-test-output.txt; then
        echo "$(date +%H:%M:%S) Attempt $ATTEMPTS: Lock still held, waiting 60s..." >> "$RESULT_FILE"
        sleep 60
        continue
    fi

    echo "EXIT $EXIT_CODE" >> "$RESULT_FILE"
    tail -60 /tmp/kcode-test-output.txt >> "$RESULT_FILE"
    echo "COMPLETE" >> "$RESULT_FILE"
    exit $EXIT_CODE
done
