#!/bin/bash

# Open the generated PGN Rust code for a quick inspection.

cd $DEV_PATH/warehouse/projects/professional/korrigan/libs/korri-n2k/
find ../../target -name "generated_pgns.rs" -exec hx {} \;
