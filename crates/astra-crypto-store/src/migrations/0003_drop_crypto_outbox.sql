-- ECS (the crypto engine seam): the Astra-side crypto outbox is retired.
-- Crypto sends ride the engine's own outbox through the MIME-wrap seam
-- (engine_provider::MimeWrapper), so the one queue owns every send again.
-- SP4b shipped to no release, so dropping the table abandons no user data
-- a released build could have written.

DROP TABLE IF EXISTS crypto_outbox;
