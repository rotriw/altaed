#!/usr/bin/env python3
"""Compatibility entry point: prepare the editor dependency for our workspace."""
import argparse
from pathlib import Path
from prepare_zed import prepare

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, help='Optional local Zed git checkout')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    prepare(root, args.source.resolve() if args.source else None)
    print(root / 'Cargo.toml')


if __name__ == '__main__':
    main()
