# Mutirão

Orquestrador de agentes de IA num canvas infinito, para trabalho geral — não só
código. Windows 11, Tauri 2 + React.

**Estado: M0 a M5 prontos.** Canvas com pan e zoom, nós arrastáveis e
redimensionáveis, cabos entre eles, tudo persistido em SQLite (M0). Nó de agente
conversando com o **Claude Code de verdade**: face conversa e face terminal,
turno com cancelamento, ações como cards, retomada da sessão e custo por turno e
por workspace (M1). Notas que viram `.md` na sua pasta, árvore de arquivos de
verdade, e o agente gravando **só depois de você aprovar** (M2). Um agente
**fala com outro**: o Pesquisador pede ao Redator e devolve a resposta, sem
você tocar (M3). Um prompt **monta o time**: papéis com prompt e ferramentas
próprias, um Organizador que recruta quem falta, e o time inteiro salvo para
reabrir amanhã (M4). E agora **rascunhos**: duas versões do mesmo trabalho
rodando ao mesmo tempo, e publicar uma delas sem você ver uma linha de Git (M5).

> **Um nó só enxerga o que os cabos deixam.** Ligou `fala_com`? Pode mandar
> recado. Não ligou? Aquele nó **não existe** — e a mensagem de erro é a mesma
> de um nó que nunca existiu, para tentativa nenhuma virar sonda do seu canvas.
> Quando dois nós conversam, o cabo acende: a ponte é visível, não mágica.

> **Nada é gravado sem o seu clique.** Antes de escrever qualquer arquivo ou
> rodar qualquer comando, o agente para num card que mostra o quê e quanto —
> e fica parado até você decidir. Não é gravar e desfazer: não chega a gravar.
> Para gravação você pode dizer "não perguntar de novo nesta pasta"; para rodar
> comando, não — isso pergunta sempre.

Precisa do [Claude Code](https://code.claude.com) instalado e autenticado, e do
Git para os rascunhos. Sem qualquer um dos dois o app sobe assim mesmo — no
adaptador falso (roteiro em vez de modelo) e sem rascunho — e **diz isso na
barra de cima**, nunca em silêncio. A CLI é procurada no PATH como o console
procuraria, então o `claude.cmd` que o npm instala no Windows é encontrado;
`MUTIRAO_CLAUDE_BIN` continua mandando quando você quer apontar outra.

```bash
npm install
npm run dev      # front no navegador, com núcleo falso (nada é gravado)
npm run app      # app de verdade

MUTIRAO_ADAPTADOR=falso npm run app   # força o roteiro: mexer na interface sem gastar
MUTIRAO_CLAUDE_BIN=...  npm run app   # CLI fora do PATH (comum no Windows)
```

Testes:

```bash
cargo test -p nucleo        # 136 testes, offline e de graça
node testes-ui/fumaca.mjs   # 65 verificações da interface no Chromium

# Estes gastam token e precisam da CLI instalada. Rode ao subir de versão dela.
# Os do M3 ao M5 sobem VÁRIOS Claude Code, falando entre si e em rascunhos.
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
136 testes, inclusive os de turno completo, os da ponte, os do time e os de rascunho.

O adaptador falso (`nucleo/src/agente.rs`) não é conveniência de teste: testar
orquestração contra a API de verdade é lento, caro e não-determinístico. Ele lê
um roteiro e emite exatamente os mesmos eventos que o adaptador Claude
(`nucleo/src/claude.rs`) emite a partir do JSONL da CLI.

O barramento (`nucleo/src/barramento.rs`) é um servidor em `127.0.0.1` com
escopo por token: token → sessão → nó → workspace. É por ele que o agente pede
licença, e é ele que segura a chamada enquanto o card espera um clique. Desde o
M3 ele também serve o MCP (`nucleo/src/mcp.rs`) em `/mcp` — mesma porta, mesmo
token, mesmo processo. Um segundo canal seria um segundo escopo para manter em
dia, e escopo mantido em dois lugares diverge.

As ferramentas que o agente enxerga estão em `nucleo/src/ferramentas.rs`, e o
que cada uma alcança é decidido pelos cabos. Duas delas gravam em disco, e as
duas passam pelo **mesmo card do M2**: medido na CLI 2.1.251, o hook
`PreToolUse` dispara para ferramenta MCP também, e negado o `tools/call` nunca
chega ao servidor. Não é gravar e desfazer — não chega a gravar.

Três coisas que a medição decidiu, e que estão explicadas em
`ESPECIFICACAO.md §9`: o Claude roda pela **CLI headless**, sem sidecar Node; o
**custo vem do `total_cost_usd` da própria CLI**, porque uma tabela de preços
que ignora cache erra por quase 12 vezes; e a aprovação sai por um **hook
`PreToolUse` do tipo HTTP**, porque o `--permission-prompt-tool` que o plano
previa não existe mais.

Uma quarta, que só apareceu escrevendo o M3: os três limites do plano — saltos,
prazo e orçamento — não pegam a **espera cruzada**, quando A está parado
esperando B e B pergunta de volta a A. Saltos só contam quando alguém anda,
orçamento só soma quando alguém gasta, e o prazo pega em dez minutos, que para
quem está olhando a tela é travar. Por isso o orquestrador segue a corrente de
quem-espera-quem e recusa na hora, dizendo ao modelo o que fazer em vez disso.

E uma quinta, no M4: nenhum dos quatro impede um agente de **recrutar** cem
outros, porque recrutar não é salto nem gasto de mensagem. Entraram um teto por
cadeia e outro por workspace.

## Papéis

Um papel é prompt de sistema + ferramentas + autonomia. Vêm cinco no app —
Pesquisador, Redator, Revisor, Analista e Organizador —, e o papel escolhido
aparece no cabeçalho de cada nó.

**Autonomia escolhe ferramentas, nunca permissões.** `cauteloso` só lê e
conversa; `padrao` grava, com card; `solto` também roda comando, com card
sempre. Nenhum nível dispensa a aprovação — um que dispensasse seria o "pular
todas as permissões" que o `ARQUITETURA.md §8` proíbe, com outro nome.

O Organizador monta time com `recrutar`. Quem ele recruta nasce ligado a ele e
com papel; `dispensar` encerra a sessão e **não apaga o nó** — apagar levaria a
conversa junto, e quem apaga nó é você.

**Salvar time** guarda quem trabalha e como está ligado, para reabrir amanhã.
Não guarda a conversa: partitura é a planta do time, não um backup dele.

## Rascunhos

Um rascunho é uma cópia isolada da pasta em que o time trabalha sem mexer no que
está valendo. Dois deles rodam ao mesmo tempo, com versões diferentes do mesmo
arquivo, e a pasta de verdade só muda quando você publica.

**Publicar mostra antes.** Quantos arquivos mudam, quais conflitam, e de qual
lado ficar em cada conflito — nada vem pré-marcado. É o mesmo padrão do card de
aprovação, e pelo mesmo motivo: reescrever arquivo na pasta de alguém não se
desfaz, se mostra antes.

Por baixo é Git, e **você nunca vê isso** — nem uma palavra de branch, commit ou
merge, o que o teste de fumaça confere varrendo a tela. O repositório fica fora
da sua pasta, na pasta de dados do app: a sua continua com os arquivos do
trabalho e mais nada. Isso não é só estética — pasta de trabalho no Windows quase
sempre está no OneDrive, e diretório Git dentro de pasta sincronizada corrompe.

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
