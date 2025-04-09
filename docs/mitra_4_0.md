# Mitra 4.0 migration guide

## Preparation

If you're using Mitra 2.x, read the [Mitra 3.0 migration guide](./mitra_3_0.md). Updating from 2.x to 4.x is not supported.

## General

- Removed unhashed OAuth tokens from the database. Some users might be logged out as a result.

## Web client

- Minimum supported version of NodeJS is 20.
