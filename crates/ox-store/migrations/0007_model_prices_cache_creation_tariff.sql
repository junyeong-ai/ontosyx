-- Add the cache-creation input tariff column to `model_prices`.
-- Providers charge a per-million premium when a dispatch establishes
-- a new prompt-cache breakpoint (Anthropic: ~1.25× the cache-miss
-- input rate; OpenAI / Bedrock vary). The cost arithmetic now sums
-- five legs — full-rate input, cache-read input, cache-creation
-- input, output, reasoning — and the per-million tariffs match.
--
-- DEFAULT 0 is safe (the column never contributed to existing cost
-- arithmetic). Admins update the tariff per row through the model-
-- prices admin surface when they configure a fresh provider.

ALTER TABLE model_prices
    ADD COLUMN cache_creation_input_price_usd_per_million DOUBLE PRECISION NOT NULL DEFAULT 0;
