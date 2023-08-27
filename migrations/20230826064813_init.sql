CREATE TABLE IF NOT EXISTS block_tasks
(
    block_number INTEGER PRIMARY KEY NOT NULL
);

CREATE TABLE IF NOT EXISTS tx_tasks
(
    tx_hash      BLOB PRIMARY KEY NOT NULL
);

CREATE TABLE IF NOT EXISTS opcode_statistics
(
    block_number INTEGER NOT NULL,
    opcode       INTEGER NOT NULL,
    count        INTEGER NOT NULL,
    UNIQUE (block_number, opcode)
);

CREATE INDEX IF NOT EXISTS idx_opcode_statistics_block_number ON opcode_statistics (block_number);
CREATE INDEX IF NOT EXISTS idx_opcode_statistics_opcode ON opcode_statistics (opcode);
