# simple_include_counter
Very simple include analyzer.

## How it works
1. Reads all `.h`/`.cpp` files from given directories
2. Looks for `#include xxx` directives
3. Builds an include tree
4. Analyzes the tree
5. Prints to stdout in LibreOffice-friendly way (simply copy-paste into Calc)

## Extracted info for each file
1. Size in bytes / lines / lines of code
2. List of includees, sorted in most-included order
3. Total direct/indirect inclusions
4. Contribution to compilation, both self and self + includes

## Pain points
1. Ignores preprocessor macros, but repsects comments
2. Does not seek out system/missing headers, assumes each to be 1 code line long with no includes
3. Fails if there's a recursive dependency

## Usage
```
cargo run --release -- input/dir/1/ input/dir/2/ input/dir/3/ > ~/Desktop/report.txt
```
Then copy contents of `report.txt` and paste into LibreOffice Calc.
