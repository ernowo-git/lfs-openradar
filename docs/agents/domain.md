# Domain Docs

This repo uses a single-context layout: `GLOSSARY.md` at the root and ADRs under `docs/adr/`.

## Before exploring

Read the root `GLOSSARY.md` and ADRs that touch the area being explored. If a root `GLOSSARY-MAP.md` exists later, follow it to the relevant context glossaries and also check their `src/<context>/docs/adr/` directories.

If these files do not exist, proceed silently. Do not flag their absence or suggest creating them upfront. The domain-modeling skill creates them lazily when terms or decisions get resolved.

## Vocabulary and decisions

Use glossary terms in issue titles, proposals, hypotheses, and test names. Avoid synonyms the glossary explicitly rejects. If a needed concept is missing, reconsider the term or note the gap for domain-modeling.

Surface conflicts with existing ADRs explicitly rather than silently overriding them; identify the ADR and explain why the decision may need revisiting.
