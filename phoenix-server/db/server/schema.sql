CREATE TABLE IF NOT EXISTS system_stats (
    id SERIAL PRIMARY KEY,
    cpu_usage REAL,
    memory_used BIGINT,
    memory_total BIGINT,
    timestamp TIMESTAMPTZ
);
