#!/bin/sh
# This regenerates the D2 diagrams in the Antora documentations (requires d2 executable in path)
process_files() {
  local DIR="$1"
  for FILE in "$DIR"/*.d2; do
    if [ -f "$FILE" ]; then
      FILE_NAME=$(basename "$FILE")
      echo "Processing $FILE_NAME"
      FILE_NAME_NO_EXT="${FILE_NAME%.*}"
      d2 --sketch "$FILE" "$DIR/target/$FILE_NAME_NO_EXT.svg"
    fi
  done
}

process_files "doc/realearn/modules/ROOT/images/realearn/diagrams/control-flow"