# Mutirão

Orquestrador de agentes de IA num canvas infinito, para trabalho geral — não só
código. Windows 11, Tauri 2 + React.

**Estado: M0, M1 e M2 prontos.** Canvas com pan e zoom, nós arrastáveis e
redimensionáveis, cabos entre eles, tudo persistido em SQLite (M0). Nó de agente
conversando com o **Claude Code de verdade**: face conversa e face terminal,
turno com cancelamento, ações como cards, retomada da sessão e custo por turno e
por workspace (M1). Notas que viram `.md` na sua pasta, árvore de arquivos de
verdade, e o agente gravando **só depois de você aprovar** (M2).

> **Nada é gravado sem o seu clique.** Antes de escrever qualquer arquivo ou
> rodar qualquer comando, o agente para num card que mostra o quê e quanto —
> e fica parado até você decidir. Não é gravar e desfazer: não chega a gravar.
> Para gravação você pode dizer "não perguntar de novo nesta pasta"; para rodar
> comando, não — isso pergunta sempre.

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
cargo test -p nucleo        # 69 testes, offline e de graça
node testes-ui/fumaca.mjs   # 36 verificações da interface no Chromium

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

O barramento (`nucleo/src/barramento.rs`) é um servidor em `127.0.0.1` com
escopo por token: token → sessão → nó → workspace. É por ele que o agente pede
licença, e é ele que segura a chamada enquanto o card espera um clique.

Três coisas que a medição decidiu, e que estão explicadas em
`ESPECIFICACAO.md §9`: o Claude roda pela **CLI headless**, sem sidecar Node; o
**custo vem do `total_cost_usd` da própria CLI**, porque uma tabela de preços
que ignora cache erra por quase 12 vezes; e a aprovação sai por um **hook
`PreToolUse` do tipo HTTP**, porque o `--permission-prompt-tool` que o plano
previa não existe mais.

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
