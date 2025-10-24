#!/bin/bash

# Utility script: open the generated lookup file for inspection.

cd $DEV_PATH/warehouse/projects/professional/korrigan/libs/korri-n2k/
find ../../target -name "generated_lookups.rs" -exec hx {} \;
