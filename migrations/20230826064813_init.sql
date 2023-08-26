CREATE TABLE IF NOT EXISTS block_status
(
    block_number INTEGER PRIMARY KEY,
    tx_fetched   INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS tx_status
(
    block_number INTEGER,
    tx_index     INTEGER,
    tx_hash      BLOB PRIMARY KEY,
    tx_analyzed  INTEGER DEFAULT 0,
    FOREIGN KEY (block_number) REFERENCES block_status (block_number)
);

CREATE INDEX IF NOT EXISTS idx_tx_status_block_number ON tx_status (block_number);

CREATE TABLE IF NOT EXISTS opcode_statistics
(
    block_number INTEGER,
    opcode       INTEGER,
    count        INTEGER,
    FOREIGN KEY (block_number) REFERENCES block_status (block_number),
    UNIQUE (block_number, opcode)
);

CREATE INDEX IF NOT EXISTS idx_opcode_statistics_block_number ON opcode_statistics (block_number);