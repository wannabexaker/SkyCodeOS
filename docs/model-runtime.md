# Model Runtime and Router (V1)

## Runtime Targets

- local: llama.cpp (GGUF)
- remote: OpenAI-compatible adapter

## Router Policy

- simple classification/summarization -> small local model
- code-edit/refactor -> local coder model
- complex architectural reasoning -> strong model with fallback

## Fallback Chain

1. local primary
2. local fallback
3. remote strong
4. fail with explicit reason

## Operational Notes

- Keep provider adapters isolated behind Model Runtime contract.
- Maintain model registry (name, context, speed, strengths, cost).
