CREATE TABLE IF NOT EXISTS block
(
    block_number INTEGER PRIMARY KEY NOT NULL,
    tx_fetched   INTEGER DEFAULT 0 NOT NULL
);

CREATE TABLE IF NOT EXISTS tx
(
    block_number INTEGER NOT NULL,
    tx_index     INTEGER NOT NULL,
    tx_hash      BLOB PRIMARY KEY NOT NULL,
    tx_analyzed  INTEGER DEFAULT 0 NOT NULL,
    FOREIGN KEY (block_number) REFERENCES block (block_number)
);

CREATE INDEX IF NOT EXISTS idx_tx_status_block_number ON tx (block_number);

CREATE TABLE IF NOT EXISTS opcode_statistics
(
    block_number INTEGER NOT NULL,
    opcode       INTEGER NOT NULL,
    count        INTEGER NOT NULL,
    FOREIGN KEY (block_number) REFERENCES block (block_number),
    UNIQUE (block_number, opcode)
);

CREATE INDEX IF NOT EXISTS idx_opcode_statistics_block_number ON opcode_statistics (block_number);