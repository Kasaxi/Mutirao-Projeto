# Mutirão

Orquestrador de agentes de IA num canvas infinito, para trabalho geral — não só
código. Windows 11, Tauri 2 + React.

**Estado: M1, menos o adaptador Claude.** Canvas com pan e zoom, nós
arrastáveis e redimensionáveis, cabos entre eles, tudo persistido em SQLite
(M0). Nó de agente com face conversa e face terminal, turno com cancelamento,
ações do agente como cards e custo por turno e por workspace (M1).

> ⚠️ **As respostas vêm de um roteiro, não de um modelo.** O adaptador em uso é
> o falso: nenhum token é gasto e nada é enviado para lugar nenhum. O app diz
> isso na barra de cima. Trocar pelo adaptador Claude é a próxima peça — só a
> `Fabrica` em `src-tauri/src/main.rs` precisa mudar.

```bash
npm install
npm run dev      # front no navegador, com núcleo falso (nada é gravado)
npm run app      # app de verdade
```

Testes:

```bash
cargo test -p nucleo        # 43 testes do núcleo
node testes-ui/fumaca.mjs   # 27 verificações da interface no Chromium
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
um roteiro e emite os mesmos eventos que o adaptador de verdade emitirá.

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
