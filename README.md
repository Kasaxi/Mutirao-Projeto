# Mutirão

Orquestrador de agentes de IA num canvas infinito, para trabalho geral — não só
código. Windows 11, Tauri 2 + React.

**Estado: M0 e M1 prontos.** Canvas com pan e zoom, nós arrastáveis e
redimensionáveis, cabos entre eles, tudo persistido em SQLite (M0). Nó de agente
conversando com o **Claude Code de verdade**: face conversa e face terminal,
turno com cancelamento, ações como cards, retomada da sessão e custo por turno e
por workspace (M1).

> **O agente só lê.** Escrita chega no M2, junto com o card de aprovação —
> `ARQUITETURA.md §8` não admite agente que grava sem pedir licença. O turno roda
> com `--restricted` e um allowlist de leitura, confinado à pasta do workspace.

Precisa do [Claude Code](https://code.claude.com) instalado e autenticado. Sem
ele o app sobe assim mesmo, no adaptador falso (roteiro em vez de modelo), e diz
isso na barra de cima — nunca em silêncio.

```bash
npm install
npm run dev      # front no navegador, com núcleo falso (nada é gravado)
npm run app      # app de verdade

MUTIRAO_ADAPTADOR=falso npm run app   # força o roteiro: mexer na interface sem gastar
MUTIRAO_CLAUDE_BIN=...  npm run app   # CLI fora do PATH (comum no Windows)
```

Testes:

```bash
cargo test -p nucleo        # 51 testes, offline e de graça
node testes-ui/fumaca.mjs   # 27 verificações da interface no Chromium

# Estes gastam token e precisam da CLI instalada. Rode ao subir de versão dela.
cargo test -p nucleo --test ao_vivo -- --ignored --nocapture
```

Se o Chromium do Playwright não estiver onde ele espera (CI, contêiner),
aponte o binário: `CHROMIUM_BIN=/caminho/para/chromium node testes-ui/fumaca.mjs`.

## Onde ler o quê

| Arquivo | Para quê |
|---|---|
| `ARQUITETURA.md` | as decisões e o porquê delas; os marcos M0–M6 |
| `ESPECIFICACAO.md` | contratos exatos: IPC, MCP, esquema, telas, convenções |

## Como está organizado

O núcleo (`nucleo/`) é um crate Rust puro: modelo, banco, regras, adaptadores e
orquestração, sem nada de interface. O shell (`src-tauri/`) é casca fina —
janela, IPC e a tradução de evento do núcleo em evento de janela. O front
(`src/`) desenha o canvas e nunca fala com o backend fora de `src/lib/ipc.ts`.

Essa separação existe por um motivo prático: `cargo test -p nucleo` roda em
qualquer máquina, sem as dependências de sistema do Tauri — e é onde estão os
43 testes, inclusive os de turno completo.

O adaptador falso (`nucleo/src/agente.rs`) não é conveniência de teste: testar
orquestração contra a API de verdade é lento, caro e não-determinístico. Ele lê
um roteiro e emite exatamente os mesmos eventos que o adaptador Claude
(`nucleo/src/claude.rs`) emite a partir do JSONL da CLI.

Duas coisas que a medição decidiu, e que estão explicadas em
`ESPECIFICACAO.md §9`: o Claude roda pela **CLI headless**, sem sidecar Node; e
o **custo vem do `total_cost_usd` da própria CLI**, porque uma tabela de preços
que ignora cache erra por quase 12 vezes.

## Licença e uso

Proprietário, todos os direitos reservados — veja `LICENSE`. Repositório privado,
uso interno, sem distribuição. As quatro decisões que fixam isso (nome, licença,
chave de API, cobrança) estão registradas em `ESPECIFICACAO.md §11`, com o que
cada uma manda fazer e o que ela adia.

**A chave da API nunca entra no repositório.** A partir do M1 o adaptador Claude
a lê do ambiente (`ANTHROPIC_API_KEY`); o app não a grava em disco, nem no banco,
nem em log. O `.gitignore` cobre `.env*`, mas isso é rede de proteção, não
permissão: repositório privado não é cofre, e o histórico do Git não esquece.

## Ícone

Já existe um provisório em `src-tauri/icons/` — três nós ligados, porque a
marca é a coordenação, não a caixa. Para trocar pela definitiva:

```bash
npx tauri icon caminho/para/logo.png
```
