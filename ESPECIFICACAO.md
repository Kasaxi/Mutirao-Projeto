# Mutirão — Especificação de implementação

Companheiro do `ARQUITETURA.md`. Aquele diz **o quê** e **por quê**; este diz
**onde** e **como**, com contratos exatos. Um agente de código deve conseguir
abrir este arquivo e escrever a próxima função sem inventar nome, caminho ou
formato.

Estado atual: **M0 a M5 prontos e testados** contra o Claude Code de verdade —
sessão, turno, cancelamento, custo, retomada, duas faces (M1); barramento local
com card de aprovação, notas em arquivo e árvore da pasta (M2); servidor MCP,
ponte entre nós e os limites da cadeia (M3); papéis, Maestro Mode e times
salvos (M4); rascunhos sobre Git oculto, tela de publicar e host MCP (M5). O
portal de navegador do M5 e o M6 ainda são contrato, não código.

---

## 1. Estrutura do repositório

```
mutirao/
├── ARQUITETURA.md          decisões e marcos (o porquê)
├── ESPECIFICACAO.md        este arquivo (o como)
├── Cargo.toml              workspace Rust: nucleo + src-tauri
├── package.json            front e scripts
├── vite.config.ts
├── tsconfig.json
├── index.html
│
├── nucleo/                 CRATE PURO — sem Tauri, sem UI
│   ├── Cargo.toml
│   ├── migrations/
│   │   ├── 001_inicial.sql        esquema completo, executável
│   │   ├── 002_adaptador_falso.sql  'falso' no CHECK de session.adaptador
│   │   ├── 003_regras_de_aprovacao.sql  a caixa "não perguntar de novo"
│   │   ├── 004_papeis.sql        papel no nó, modelo por papel, quem recrutou
│   │   ├── 005_ensaios.sql       onde mora o histórico oculto
│   │   └── 006_mcp_externo.sql   servidores MCP externos por papel
│   ├── testes/
│   │   └── claude_stream.jsonl  saída REAL da CLI, guardada como fixture
│   ├── tests/
│   │   └── ao_vivo.rs      testes #[ignore] que rodam o Claude Code de verdade
│   └── src/
│       ├── lib.rs          fachada + 128 testes
│       ├── modelo.rs       tipos de domínio, máquina de estados, preços
│       ├── agente.rs       trait AgenteAdapter, Roteiro, adaptador falso
│       ├── claude.rs       adaptador do Claude Code (CLI headless)
│       ├── barramento.rs   servidor local, escopo por token, aprovação
│       ├── mcp.rs          servidor MCP em /mcp, JSON-RPC 2.0
│       ├── ferramentas.rs  as ferramentas do §6, com escopo pelos cabos
│       ├── papeis.rs       biblioteca embutida e escada de autonomia
│       ├── partituras.rs   fotografar e montar um time salvo
│       ├── git.rs          o Git oculto: repo fora da pasta, merge seco
│       ├── ensaios.rs      rascunhos: criar, trocar, publicar, descartar
│       ├── arquivos.rs     escopo de caminho, listar, ler e gravar
│       ├── orquestrador.rs turno, fila, ponte entre nós, limites
│       ├── db.rs           migrations e todo o acesso a dados
│       └── erro.rs         Erro, códigos estáveis
│
├── src-tauri/              CASCA — janela, IPC, ciclo de vida
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs         setup, registro de comandos
│       ├── comandos.rs     um #[tauri::command] por operação
│       ├── estado.rs       EstadoApp (Mutex<Banco>)
│       └── erro.rs         ErroIpc { codigo, mensagem }
│
├── src/                    FRONT
│   ├── main.tsx
│   ├── App.tsx             orquestra canvas, gestos, teclado
│   ├── estilo.css          tokens e componentes
│   ├── lib/
│   │   ├── tipos.ts        espelho dos tipos do Rust
│   │   ├── ipc.ts          ÚNICO lugar que fala com o backend
│   │   └── adiar.ts        debounce de gravação
│   └── canvas/
│       ├── viewport.ts     matemática de pan/zoom, enquadrar
│       ├── NoView.tsx      um nó
│       ├── Conversa.tsx    face conversa e face terminal
│       ├── Rascunhos.tsx   barra de rascunhos e tela de publicar
│       └── Cabos.tsx       SVG dos cabos
│
└── testes-ui/
    ├── fumaca.mjs          66 verificações no Chromium
    ├── canvas.png          retrato do canvas em repouso
    ├── conversa.png        retrato de um turno inteiro
    ├── aprovacao.png       o card aberto, com o turno parado atrás
    ├── ponte.png           a ponte no ato de atravessar
    ├── time.png            o time montado, com papel em cada nó
    └── publicar.png        a tela de publicar, em linguagem de gente
```

**Regra de ouro:** nenhuma regra de negócio em `src-tauri/`. Se um comando
tem mais que uma linha de lógica, ela pertence ao `nucleo`. O motivo é
testabilidade — `cargo test -p nucleo` roda em qualquer máquina, inclusive CI
Linux, sem as dependências de sistema do Tauri.

---

## 2. Ambiente e execução

### Pré-requisitos no Windows

| O quê | Por quê |
|---|---|
| Rust estável (rustup) | núcleo e shell |
| Node 20+ | front e ferramentas |
| Visual Studio Build Tools + Windows SDK | linkagem do Tauri |
| WebView2 Runtime | já vem no Windows 11 |
| Git for Windows | **os rascunhos do M5 precisam dele**; sem ele o app roda sem rascunho e diz isso na barra. Também é o que a ferramenta Bash do Claude Code usa |

### Comandos

```bash
npm install                 # dependências do front
npm run dev                 # front sozinho, no navegador, com núcleo falso
npm run app                 # app de verdade (tauri dev)
npm run build               # typecheck + build do front
npm run app:build           # instalador MSI/NSIS

cargo test -p nucleo        # 128 testes do núcleo, offline e de graça
node testes-ui/fumaca.mjs   # teste de fumaça da interface
```

O teste de fumaça usa o Chromium que vem com o Playwright. Em máquina onde ele
não está onde o Playwright espera — CI, contêiner — aponte o binário:
`CHROMIUM_BIN=/caminho/para/chromium node testes-ui/fumaca.mjs`.

Os ícones já estão em `src-tauri/icons/` (marca provisória: três nós ligados).
`tauri::generate_context!` os exige em tempo de compilação — sem eles o build
falha, não é opcional. Para trocar pela marca definitiva:
`npx tauri icon caminho/para/logo.png`, que regenera a pasta inteira, inclusive
o `.ico` que o instalador do Windows precisa.

### Modo navegador

`npm run dev` fora do Tauri não tem backend. Em vez de quebrar, `src/lib/ipc.ts`
cai num núcleo falso em memória, com as mesmas regras e um canvas de exemplo.
Serve para iterar interface rápido e para o teste de fumaça rodar em CI.

O falso **copia tudo que devolve** (`structuredClone`), porque o IPC de verdade
serializa. Sem isso, front e "backend" compartilham objeto e bugs de aliasing
aparecem só em desenvolvimento — foi exatamente o que aconteceu na primeira
rodada do teste: um cabo criado contava como dois.

---

## 3. Contrato IPC

Tauri converte os nomes dos argumentos de `camelCase` (JS) para `snake_case`
(Rust) automaticamente. Por isso `workspaceId` no TypeScript chega como
`workspace_id` no comando. **Não** renomeie um lado sem o outro.

Erro é sempre `{ codigo, mensagem }`. Códigos estáveis: `banco`, `json`, `io`,
`nao_encontrado`, `invalido`, `fora_do_escopo`. Erros internos (banco, io, json)
nunca vazam o texto original para a interface — viram uma frase genérica e o
detalhe vai para o stderr.

| Comando | Argumentos | Devolve | Erros esperados |
|---|---|---|---|
| `criar_workspace` | `nome, pasta` | `Workspace` | `invalido` (nome vazio) |
| `listar_workspaces` | — | `Workspace[]` | — |
| `abrir_workspace` | `workspaceId` | `EstadoCanvas` | `nao_encontrado` |
| `salvar_viewport` | `workspaceId, x, y, zoom` | — | `invalido` (zoom ≤ 0), `nao_encontrado` |
| `criar_no` | `workspaceId, tipo, nome, x, y` | `No` | `nao_encontrado` |
| `mover_no` | `id, x, y, w, h` | — | `invalido` (NaN, w/h ≤ 0), `nao_encontrado` |
| `renomear_no` | `id, nome` | — | `invalido`, `nao_encontrado` |
| `trazer_para_frente` | `id` | `number` (novo z) | `nao_encontrado` |
| `remover_no` | `id` | — | `nao_encontrado` |
| `criar_cabo` | `workspaceId, deNode, paraNode, tipo` | `Cabo` | `invalido` (auto-ligação, duplicado) |
| `remover_cabo` | `id` | — | `nao_encontrado` |
| `abrir_sessao` | `nodeId` | `Sessao` | `nao_encontrado`, `invalido` (nó não é agente) |
| `adaptador_em_uso` | — | `{ adaptador, detalhe }` | — |
| `decidir_aprovacao` | `toolCallId, decisao, lembrar` | — | `nao_encontrado` (já decidido), `invalido` (regra para ferramenta que pergunta sempre) |
| `aprovacoes_pendentes` | `sessionId` | `PedidoAprovacao[]` | — |
| `listar_regras` | `workspaceId` | `RegraAprovacao[]` | — |
| `revogar_regra` | `id` | — | `nao_encontrado` |
| `listar_pasta` | `workspaceId, sub` | `ItemArquivo[]` | `fora_do_escopo`, `io` |
| `ler_nota` | `nodeId` | `{ arquivo, conteudo }` | `invalido` (nó não é nota) |
| `escrever_nota` | `nodeId, conteudo` | — | `fora_do_escopo`, `io` |
| `sessao_do_no` | `nodeId` | `Sessao \| null` | — |
| `enviar_mensagem` | `sessionId, texto` | — | `invalido` (vazia, turno em andamento), `nao_encontrado` |
| `cancelar_turno` | `sessionId` | — | `nao_encontrado` |
| `historico` | `sessionId, limite` | `Mensagem[]` | — |
| `listar_papeis` | — | `Papel[]` | — |
| `criar_papel` | `nome, prompt, ferramentas, autonomia, modelo` | `Papel` | `invalido` (nome repetido, prompt vazio) |
| `editar_papel` | `id, prompt, ferramentas, autonomia, modelo` | `Papel` | `nao_encontrado`, `invalido` |
| `remover_papel` | `id` | — | `invalido` (embutido), `nao_encontrado` |
| `quantos_usam_o_papel` | `id` | `number` | — |
| `definir_papel_do_no` | `nodeId, roleId \| null` | `No` | `nao_encontrado` |
| `salvar_time` | `workspaceId, nome` | `Partitura` | `invalido` (nome vazio, canvas vazio) |
| `listar_times` | `workspaceId` | `Partitura[]` | — |
| `abrir_time` | `workspaceId, partituraId` | `No[]` | `nao_encontrado`, `invalido` (passaria do teto) |
| `remover_time` | `id` | — | `nao_encontrado` |
| `listar_rascunhos` | `workspaceId` | `Ensaio[]` | — |
| `criar_rascunho` | `workspaceId, nome` | `Ensaio` | `invalido` (sem histórico, nome repetido) |
| `trocar_rascunho` | `workspaceId, ensaioId \| null` | — | `invalido` (já publicado), `nao_encontrado` |
| `descartar_rascunho` | `ensaioId` | — | `nao_encontrado` |
| `prever_publicacao` | `ensaioId` | `PreviaPublicacao` | `nao_encontrado`, `invalido` (sem histórico) |
| `publicar_rascunho` | `ensaioId, escolhas` | `PreviaPublicacao` | `invalido` (conflito sem escolha, já publicado) |
| `definir_mcp_do_papel` | `id, servidores` | `Papel` | `invalido` (nome com curinga, sem endereço) |
| `acoes_da_sessao` | `sessionId` | `ChamadaFerramenta[]` | — |
| `custo_do_workspace` | `workspaceId` | `{ total, por_no }` | — |

`abrir_workspace` devolve o estado inteiro numa viagem só. Abrir um canvas não
pode custar três chamadas de IPC.

`abrir_sessao` devolve a sessão que já existir naquele nó. Reabrir o app
continua a conversa; não começa outra. **Não recebe `adaptador`**: quem decide
qual agente responde é o backend, que é quem procurou a CLI na máquina. Front
que escolhe isso acaba anunciando na barra um agente diferente do que respondeu.

`adaptador_em_uso` existe para a barra dizer a verdade. Um app que conversa com
um roteiro e não avisa é uma mentira; um que conversa com um modelo e não avisa
é uma conta-surpresa.

### Eventos Rust → front

Emitidos desde o M1. **Payload em `snake_case`**, como todo o resto que
atravessa a fronteira — a tabela antiga dizia `sessionId`, e manter duas
convenções de nome no mesmo canal é exatamente a "duas verdades" que a §10
proíbe. Todo payload carrega também `tipo`, para o front discriminar sem
depender só do nome do evento.

| Evento | Payload | Quando | Estado |
|---|---|---|---|
| `sessao:evento` | `{ tipo, session_id, evento: EventoAgente }` | a cada evento do adaptador | pronto |
| `sessao:estado` | `{ tipo, session_id, node_id, estado, pede_atencao }` | mudança na máquina de estados | pronto |
| `custo:atualizado` | `{ tipo, workspace_id, total, por_no }` | fim de turno | pronto |
| `aprovacao:pedida` | `{ tipo, pedido: PedidoAprovacao }` | ferramenta exige aprovação | pronto |
| `aprovacao:decidida` | `{ tipo, tool_call_id, node_id, decisao, decidido_por }` | alguém (ou uma regra) decidiu | pronto |
| `no:mensagem` | `{ tipo, de_node, para_node, trace_id, tipo_mensagem }` | mensagem entre nós, para animar o cabo | pronto |
| `cadeia:encerrada` | `{ tipo, trace_id, node_id, motivo }` | uma cadeia bateu num limite | pronto |

O campo é `tipo_mensagem`, e não `tipo` como a tabela antiga dizia: `tipo` já é
o discriminante do envelope, e o serde recusa o homônimo em tempo de
compilação. Vale como lembrete de que o discriminante ocupa um nome no JSON.

Regra: evento **notifica**, não carrega o histórico. O front pede o que
precisa por comando. Isso evita que a fronteira IPC vire um firehose.

**Ordem que não é acidental:** `sessao:evento` sai *antes* de o núcleo gravar o
efeito daquele evento. É de propósito — quem emite `sessao:estado` é a
gravação, e a interface reabilita o campo de escrita ao ver o estado voltar
para `ocioso`. Aplicando primeiro, o campo destravaria um instante antes de a
resposta aparecer, e o turno pareceria terminar vazio.

A consequência para quem escuta: **não reaja a um evento relendo o banco**, que
ainda não tem o que você procura. Monte a partir do próprio evento. A face
conversa faz assim, e a releitura na montagem do nó reconcilia.

---

## 4. Sessão de agente e o token de escopo

O `ARQUITETURA.md` deixou um buraco: o servidor MCP precisa saber **qual nó**
está chamando, para aplicar o escopo dos cabos. Fica assim.

Ao iniciar uma sessão, o núcleo gera um `token` opaco (32 bytes aleatórios,
hex) e grava em `session.token`. Ele viaja para o processo do agente num
arquivo de `--settings` escrito por sessão — **em arquivo, não como JSON na
linha de comando**, porque a linha de comando de um processo é legível por
qualquer outro processo do mesmo usuário, e ali dentro vai o segredo.

O que o M2 escreve nesse arquivo:

```json
{
  "hooks": {
    "PreToolUse": [{
      "matcher": "Write|Edit|NotebookEdit|Bash|WebFetch",
      "hooks": [{
        "type": "http",
        "url": "http://127.0.0.1:{porta}/aprovacao",
        "headers": { "X-Mutirao-Token": "{token}" },
        "timeout": 1860
      }]
    }]
  }
}
```

Quando o M3 acrescentar as ferramentas do §6, elas entram pela mesma porta e
com o mesmo token, aí sim como servidor MCP (`--mcp-config`).

Toda chamada de ferramenta é resolvida assim:

```
token → session → node → cabos do node → conjunto visível
```

Consequências que não são opcionais:

- O servidor escuta **só em 127.0.0.1**, e recusa requisição sem token válido.
- O token morre com a sessão. Encerrou o nó, o token não vale mais.
- Um agente não enxerga nem nomeia nós fora do seu conjunto visível — nem para
  reclamar que não achou. A resposta é "esse nó não existe", nunca "existe mas
  você não pode".
- Ninguém, em lugar nenhum, aceita um id de nó vindo do agente sem passar por
  essa resolução.

Sem essa peça, um agente que vaza o próprio prompt de sistema comanda o canvas
inteiro. Ela é do M3, mas está aqui porque não pode ser lembrada depois.

---

## 5. Ferramentas MCP

Nomes e formatos definitivos. Argumentos em `snake_case` para bater com o
resto do sistema.

```jsonc
enviar_para {
  no: string,          // nome do nó vizinho, não id
  mensagem: string,
  refs?: string[],     // notas ou arquivos citados
  prazo_ms?: number    // padrão 600000, teto 1800000
} -> { resposta: string, de: string }

avisar { no: string, mensagem: string } -> { entregue: true }

ler_nota   { nota: string } -> { conteudo: string }
escrever_nota { nota: string, conteudo: string, modo: "substituir"|"acrescentar" }
             -> { bytes: number }

listar_nos {} -> { nos: { nome, tipo, relacao }[] }

listar_arquivos { caminho?: string } -> { itens: { caminho, nome, pasta, tamanho }[] }
ler_arquivo     { caminho: string } -> { conteudo: string }
escrever_arquivo { caminho: string, conteudo: string } -> { bytes: number }

recrutar { papel: string, nome: string } -> { no: string }
dispensar { no: string } -> { encerrado: true }

perguntar_humano { pergunta: string, opcoes?: string[] } -> { resposta: string }
concluir { resumo: string } -> { ok: true }
```

Implementadas em `nucleo/src/ferramentas.rs`. Duas diferenças em relação ao
rascunho acima, e as duas nasceram de escrever o código:

- `ler_arquivo` perdeu o `truncado`. Ele prometia o que a implementação não
  entrega: `arquivos::ler_texto` **recusa** um arquivo grande demais em vez de
  cortá-lo pela metade, porque meio arquivo é pior que erro nenhum — o modelo
  não tem como saber que leu metade.
- `listar_nos` não estava na lista e precisa estar. Sem ela o agente descobre
  os vizinhos por tentativa e erro, e cada tentativa é uma sonda.

E uma sobre `dispensar`: ela **não apaga o nó**, e a palavra do rascunho já
dizia isso — "encerrado", não "removido". Apagar levaria a conversa junto por
CASCADE, e destruir trabalho por conta de um agente é o oposto do §8. Quem
apaga nó é a pessoa, com o botão que já existe.

`recrutar` e `dispensar` não passam pela escada de autonomia: quem as tem é
quem as listou no papel. Um Revisor `solto` roda comando e ainda assim não
monta time; um Organizador `padrao` monta. Montar time é função, não nível.

O agente endereça vizinhos **por nome**, não por id: id é detalhe interno e
convida a erro de cópia. A resolução nome → id acontece dentro do conjunto
visível, e nome ambíguo é erro explícito.

### Como as ferramentas chegam ao modelo

Um servidor MCP JSON-RPC 2.0 no barramento, em `/mcp` — mesma porta, mesmo
token e mesmo processo do hook de aprovação. Um segundo canal seria um segundo
escopo para manter em dia, e escopo mantido em dois lugares diverge.

O nome que o modelo vê leva o prefixo do servidor: `mcp__mutirao__enviar_para`.
No `tools/call` ele chega **sem** o prefixo — isso é coisa do lado do cliente.

Medido contra a CLI 2.1.251, com uma sonda que registrava tudo:

1. `server/discover` — **antes** do handshake, e não é do MCP: é sondagem da
   própria CLI. O `id` dela é a string `"server-discover-probe-1"`, não um
   número, e é por isso que o `id` trafega como valor JSON sem conversão.
2. `initialize`, pedindo `protocolVersion` `"2025-11-25"`. Devolvemos a versão
   que ele pediu.
3. `notifications/initialized` — sem `id`, e portanto sem resposta: 202 e corpo
   vazio. Responder a uma notificação é erro de protocolo.
4. `tools/list`, depois `tools/call`. O `params._meta` traz
   `claudecode/toolUseId`, que é o mesmo `tool_use_id` do hook e do stream.

Erro de ferramenta volta como resultado com `isError: true`, não como erro de
JSON-RPC. A diferença importa: erro de protocolo o cliente engole; resultado
com erro o **modelo lê** — e "esse nó não existe" é o que ele precisa ler para
corrigir o rumo sozinho.

### Envelope entre nós

```json
{
  "id": "msg_7f3a",
  "de": "no_pesquisa",
  "para": "no_redator",
  "tipo": "pedido",
  "corpo": "revise a seção de garantias",
  "refs": ["nota_briefing"],
  "trace": "tr_91c2",
  "saltos": 2,
  "prazo_ms": 600000
}
```

Limites do host, não do agente: **6 saltos**, prazo por mensagem (padrão 10
min, teto 30), orçamento de **US$ 1,00 por `trace`**. Estourou qualquer um: a
cadeia encerra, quem pediu recebe erro, e o usuário vê um aviso
(`cadeia:encerrada`) — nunca um silêncio.

### O quarto limite, que o plano não previa: a espera cruzada

Os três acima não cobrem o caso pior. A saber: A manda um `pedido` a B e fica
em `aguardando_no`; B, no turno dele, manda um `pedido` de volta a A. Nenhum dos
dois pode andar.

Não é o ciclo A→B→A que o `ARQUITETURA.md §6` chama de legítimo — lá o Redator
**termina o turno** e só então o Pesquisador volta a falar. Aqui ninguém
termina nada. E os limites não pegam:

- **Saltos** não contam, porque saltos só avançam quando alguém consegue andar.
- **Orçamento** não soma, porque ninguém está gastando: os dois estão parados.
- **Prazo** pega — em dez minutos. Dez minutos com dois nós congelados é
  exatamente o travamento que o M3 promete não ter.

Então `Orquestrador::entregar` segue a corrente de quem-espera-quem antes de
enfileirar um `pedido`, e recusa na hora se ela voltar a quem está mandando. O
erro é escrito para o modelo saber o que fazer: *"o nó X está parado esperando
a SUA resposta — perguntar de volta agora trava os dois. Responda com o que
você já tem."* Um `aviso` passa: ele não espera ninguém.

O teste `dois_nos_esperando_um_pelo_outro_nao_travam_o_app` usa dois adaptadores
teimosos, que só sabem perguntar. Ele foi verificado ao contrário — com a
checagem desligada, os dois nós travam e o teste falha.

---

## 5b. Papéis, times e os tetos de crescimento

Papel = prompt de sistema + ferramentas + autonomia (+ modelo, se quiser). É o
que transforma "um agente" em "o Revisor": sem ele, quatro nós lado a lado são
o mesmo programa com nomes diferentes, e quatro iguais não são um time.

O prompt vai para o `--append-system-prompt` da CLI em **todo** turno. Medido
na 2.1.251, ele sobrevive ao `--resume` sozinho — o turno seguinte obedece sem
a flag. Passamos de novo assim mesmo, e é isso que faz uma **edição** de papel
valer para a conversa que já existe; sem repetir, mudar o prompt não teria
efeito sobre os nós que já o usam, e falharia calado.

### A escada de autonomia escolhe ferramentas, não permissões

| | lê e conversa | grava nota e arquivo | roda comando |
|---|---|---|---|
| `cauteloso` | sim | não | não |
| `padrao` | sim | sim, **com card** | não |
| `solto` | sim | sim, **com card** | sim, com card **sempre** |

Nenhum nível dispensa a aprovação. Um que dispensasse seria o "pular todas as
permissões" que o `ARQUITETURA.md §8` proíbe, com outro nome. O "sempre" da
última coluna não é ênfase: `Bash` não entra em
`FERRAMENTAS_QUE_ACEITAM_REGRA`, então nem o "não perguntar de novo" o libera.

Um papel pode **estreitar** a lista da autonomia, nunca alargá-la. Se pudesse,
`cauteloso` deixaria de querer dizer alguma coisa — e um nome que não quer
dizer nada dá confiança sem base.

**Nó sem papel continua com tudo**, que é o que o adaptador oferecia antes do
M4. Todo nó criado até aqui está assim, e estreitar o que eles alcançam seria
mudar o workspace de alguém sem ele pedir.

### O escopo do papel vale no servidor

O `--tools` da CLI esconde as ferramentas do modelo, mas **esconder não é
impedir**: um `tools/call` chega por HTTP e quem monta o corpo é o processo do
agente. A checagem que conta está em `ferramentas::executar`, antes de qualquer
efeito. Uma ferramenta fora do papel dá a mesma frase de uma que não existe,
pelo mesmo motivo do §4: "existe mas você não pode" é um mapa.

### Os tetos de recrutamento — o quinto e o sexto limite

Os quatro limites do M3 incidem sobre a **conversa**: saltos, prazo, orçamento
por cadeia, espera cruzada. Nenhum deles impede um Maestro de recrutar cem
agentes num turno só, porque recrutar não é salto nem gasto de mensagem.

- `MAX_RECRUTAS_POR_CADEIA` (6) — o teto de um pedido.
- `MAX_AGENTES_POR_WORKSPACE` (20) — o teto acumulado, para o caso que o de
  cima não cobre: três hoje, três amanhã, três depois.

`recrutar` **não** pede card. Ele não grava em disco nem destrói nada; o que
faz é gastar dinheiro, e para isso servem os tetos. Um card por recruta
transformaria "monte um time de quatro" em quatro interrupções, e interrupção
que vira rotina é interrupção que se aprova sem ler.

### Partitura é a planta do time, não um backup

Guarda nós (tipo, nome, geometria, config, papel **pelo nome**) e cabos (pelos
índices no vetor de nós). Não guarda conversa, sessão, custo nem arquivo.

Daí decorre o resto: `NoSalvo` não tem id, porque o id pertence ao canvas onde
o nó vive e reabrir cria nós **novos**; o papel vai pelo nome, porque uma
partitura precisa abrir noutra máquina onde o mesmo papel tem outro id; e
reabrir nunca sobrescreve, então abrir duas vezes dá dois times em vez de um
time corrompido pela metade. Nome repetido ganha sufixo — dois "Redator"
quebrariam o `enviar_para`, que resolve vizinho pelo nome.

---

## 5c. Rascunhos e o Git oculto

A `Decisão 3` do `ARQUITETURA.md`: "Git existe, mas o usuário nunca fica
sabendo". Implementada em `nucleo/src/git.rs` e `nucleo/src/ensaios.rs`.

### Onde o repositório mora — e por que não onde o plano dizia

**Fora da pasta do usuário**, na pasta de dados do app
(`workspace.repo`, escolhido pela casca). O plano dizia `.mutirao/` dentro,
com atributo hidden.

O motivo é específico do Windows 11: a pasta de trabalho de alguém quase sempre
está em `Documentos`, que quase sempre está sincronizada com o OneDrive — e um
diretório Git dentro de pasta sincronizada é uma forma conhecida de corromper o
repositório, porque o sincronizador mexe em arquivos que o Git presume só seus.

Fora, a pasta do usuário fica **literalmente limpa**. Medido: depois de `init` e
um commit, `ls -a` lista só os arquivos do trabalho. Os worktrees dos rascunhos
ficam ao lado do repositório, pelo mesmo motivo mais um: rascunho é trabalho
pela metade, e trabalho pela metade não deve aparecer na pasta que a pessoa abre
no Explorer.

O custo: mover a pasta do workspace desliga o histórico. Já era assim — o
caminho absoluto está gravado em `workspace.pasta` desde o M0.

### A pasta de trabalho é um lugar só

`Banco::pasta_de_trabalho(workspace)` responde "onde escrevo agora": o worktree
do rascunho em foco, ou a pasta do usuário. **Os quatro** caminhos que precisam
saber isso passam por ela — o contexto do adaptador, as ferramentas do §6, a
árvore de arquivos e as notas.

Ter dois lugares que respondem isso seria o pior bug que este projeto pode ter:
uma sessão viva apontando para o worktree errado grava no lugar errado **com
aprovação legítima do usuário**. O card diria a verdade sobre o conteúdo e
mentiria sobre o destino.

Daí decorre que `ensaios::trocar` **derruba os adaptadores vivos** antes de
mudar o ponteiro. O processo do agente recebe a pasta no `current_dir` quando
abre, e nunca mais muda de ideia; ninguém avisa o processo de nada. Os
processos morrem, as sessões ficam gravadas e o próximo turno as retoma — o
usuário perde o processo, não a conversa.

### Três coisas medidas sobre o git

1. **`git add -A`, nunca `-u`.** `-u` só pega arquivo já rastreado, e criar
   arquivo é o que um agente faz o tempo todo. Com `-u`, o parecer que o
   Redator acabou de escrever ficaria de fora do rascunho e sumiria na
   publicação — perda de trabalho silenciosa.
2. **Mesclar numa pasta com alteração não gravada não é recusado.** O git
   mescla e deixa marcador de conflito dentro do arquivo do usuário. Por isso
   `publicar` grava os dois lados antes de mesclar.
3. **`merge-tree --write-tree --name-only`** faz a mesclagem em memória e
   nomeia o que conflitaria, sem tocar em nada. É a razão de este módulo falar
   com a CLI do git em vez de libgit2: reimplementar merge de três vias para
   chegar ao mesmo lugar é trabalho de meses com muito mais chance de errar.

Sem git na máquina, `workspace.repo` fica `NULL` e o app inteiro continua
servindo — só não tem rascunho, e a barra diz isso. Perder o recurso é
aceitável; fingir que ele funciona não é.

### Publicar

`prever_publicacao` **não escreve nada**: é o que a tela mostra antes do clique.
`publicar_rascunho` grava os dois lados, mescla, e resolve cada conflito com a
escolha que o usuário fez. Conflito sem escolha **aborta tudo** e devolve a
pasta como estava: publicar pela metade é pior que não publicar.

Na tela, nenhuma palavra de Git — e há uma verificação de fumaça que varre o
texto atrás de `git`, `branch`, `commit`, `merge`, `worktree`, `rebase` e
`stash`. Promessa de produto sem teste vira lembrete.

---

## 6. Máquina de estados do turno

Implementada em `nucleo/src/modelo.rs` (`EstadoSessao::pode_ir_para`) e coberta
por teste. Transição fora desta tabela é bug do orquestrador.

| De → | ocioso | pensando | aguard. aprovação | aguard. humano | aguard. nó | erro |
|---|---|---|---|---|---|---|
| **ocioso** | — | sim | não | não | não | não |
| **pensando** | sim | — | sim | sim | sim | sim |
| **aguard. aprovação** | sim | sim | — | não | não | não |
| **aguard. humano** | não | sim | não | — | não | não |
| **aguard. nó** | não | sim | não | não | — | sim |
| **erro** | sim | não | não | não | não | — |

Pedem atenção do usuário (ponto vermelho no nó): `aguardando_aprovacao`,
`aguardando_humano`, `erro`. "Pensando" não é pedido de socorro.

Duas transições que a tabela não prevê e o M3 precisou, com
`Banco::forcar_estado_sessao` — método separado, e com esse nome, para
qualquer uso aparecer na busca:

- **`aguardando_no` → `ocioso`**, ao cancelar. Passar por `erro` seria mentir
  sobre o que houve: o usuário mandou parar, não deu problema.
- **qualquer coisa → `erro`**, quando o adaptador morre no meio. Deixar o nó
  "pensando" para sempre é o pior desfecho possível: não pede atenção, não
  aceita turno novo e não explica nada.

Fora desses dois, a checagem continua valendo — é ela que impede o orquestrador
de gravar um estado impossível.

### Um turno por vez por nó — e uma fila para quem chega no meio

Continua sendo um turno por vez. O que mudou no M3 foi o desfecho de quem
chega durante um: **fila, em ordem de chegada**, em vez de recusa. Recusar era
defensável quando só o usuário falava com o nó — ele vê a recusa e tenta de
novo. Com outro nó do outro lado, recusar é perder trabalho em silêncio.

Quem termina um turno puxa o próximo da fila. Sem essa linha, uma mensagem
enfileirada esperaria até alguém falar de novo com o nó, que é como um recado
se perde sem dar erro.

---

## 7. Telas que faltam desenhar

### Face conversa (M1)

```
┌─ AGENTE  Pesquisador ──────────────────────── ● ─┐   ● = estado
│                                                  │
│  ┌────────────────────────────────┐              │
│  │ Li os três PDFs. O item 4.2    │              │
│  │ contradiz o anexo I.           │              │
│  └────────────────────────────────┘              │
│                    ┌───────────────────────────┐ │
│                    │ confere com o contrato    │ │
│                    └───────────────────────────┘ │
│  ┌ ação ─────────────────────────────────────┐   │
│  │ 📄 leu  contrato-v3.docx          0,4 s   │   │  ações são cards,
│  └───────────────────────────────────────────┘   │  não texto de log
│                                                  │
├──────────────────────────────────────────────────┤
│ [ escreva aqui…                    ] R$ 0,42  ⟳ │   custo do turno
└──────────────────────────────────────────────────┘
```

### Card de aprovação (M2)

```
┌ PRECISA DA SUA APROVAÇÃO ────────────────────────┐
│ Gravar  orçamento-obra.xlsx                      │
│ 14 linhas alteradas na aba "Serviços"            │
│                                                  │
│ [ Ver o que mudou ]     [ Negar ]  [ Aprovar ]   │
│ ☐ não perguntar de novo para gravar nesta pasta  │
└──────────────────────────────────────────────────┘
```

A caixa "não perguntar de novo" grava uma regra em `role.ferramentas_json` e
aparece em `tool_call.decidido_por` como `regra:<nome>`. Toda permissão
concedida precisa ser visível e revogável depois.

### Publicar ensaio (M5)

```
┌ PUBLICAR "Rascunho 2" ───────────────────────────┐
│ 6 arquivos alterados, 1 conflito                 │
│                                                  │
│  ✓ minuta.docx            só neste rascunho      │
│  ✓ cronograma.xlsx        só neste rascunho      │
│  ⚠ orçamento.xlsx         mudou nos dois         │
│        ( ) manter a versão do rascunho           │
│        ( ) manter a versão original              │
│                                                  │
│ [ Cancelar ]                      [ Publicar ]   │
└──────────────────────────────────────────────────┘
```

Nenhuma palavra de Git aparece. Binário não faz merge: escolhe-se um lado.

---

## 8. Testes

| Camada | Como | Cobre |
|---|---|---|
| Núcleo | `cargo test -p nucleo` — 128 testes, offline e de graça | migrations, CRUD, escopo dos cabos e dos caminhos, validação, máquina de estados, contrato de serialização, turno inteiro, custo, cancelamento, sigilo do token, tradução do stream da CLI, aprovação e regras, handshake do MCP, ponte entre nós, fila e os limites, papéis e escada de autonomia, recrutamento com teto, partitura ida e volta, rascunhos em paralelo, publicar com e sem conflito |
| Interface | `node testes-ui/fumaca.mjs` — 66 verificações no Chromium | pan, zoom ancorado, arrastar, redimensionar, ligar, renomear, remover; um turno de ponta a ponta; o card de aprovação com aprovar, negar e "não perguntar de novo"; nota em arquivo e árvore da pasta; o cabo acendendo e o recado chegando ao outro nó com o nome de quem pediu; papel no cabeçalho, time salvo e reaberto com a forma que tinha; a barra de rascunhos e a tela de publicar — que é varrida atrás de palavra de Git |
| Ao vivo | `cargo test -p nucleo --test ao_vivo -- --ignored` — 14 testes | o Claude Code de verdade: lê, responde, cobra, retoma, reporta erro com a frase certa, grava depois de aprovado, **não** grava quando negado, **não** grava sem barramento — e, com **dois** processos, entrega de um nó a outro e encerra a cadeia sem travar; e um prompt monta um time de quatro, com o papel mudando o que cada agente de fato faz; e dois rascunhos do mesmo trabalho guardando versões diferentes ao mesmo tempo |

**Duas pastas parecidas, de propósito.** `nucleo/testes/` guarda fixtures (nome
em português, como o resto); `nucleo/tests/` é a pasta que o Cargo exige em
inglês para testes de integração. Não unifique — o Cargo não deixa.

Os testes ao vivo são `#[ignore]` porque gastam dinheiro e precisam de rede. Um
`cargo test` normal continua offline e determinístico, que é a razão de o
adaptador falso existir. Rode-os **ao subir de versão da CLI**: é lá que a forma
dos eventos muda, e quando muda, os testes de fixture continuam passando
sozinhos e felizes.

O fixture `claude_stream.jsonl` é saída **capturada da CLI 2.1.251**, não uma
invenção do que ela deveria devolver. Testar tradução contra JSON inventado só
prova que sabemos escrever o que já escrevemos.

O adaptador falso é obrigatório, não conveniência: testar orquestração contra a
API de verdade é lento, caro e não-determinístico. Ele lê um roteiro
(`{ atraso_ms, eventos: [...] }`) e emite os mesmos `EventoAgente`. Existe em
duas encarnações — `nucleo/src/agente.rs` para o app e os testes do núcleo, e
um espelho em `src/lib/ipc.ts` para o modo navegador, que não tem Rust por
baixo. A duplicação é conhecida e vale o preço: sem ela não dá para desenvolver
nem testar a interface fora do Tauri.

Alguns testes valem por si, porque cobrem coisa que falha calada:

- `o_token_do_mcp_nunca_sai_no_json_da_sessao` — o segredo do §4 atravessando a
  fronteira seria a falha de segurança mais barata de cometer e a mais difícil
  de notar.
- `adaptador_que_cala_no_meio_deixa_o_no_em_erro_e_nao_pensando` — nó preso em
  "pensando" não pede atenção, não aceita turno novo e não explica nada.
- `migration_002_reconstroi_session_sem_perder_os_filhos` — reconstruir tabela
  com FK ligada apaga os filhos por CASCADE sem erro nenhum.
- `link_simbolico_para_fora_tambem_e_recusado` — recusar `..` por texto deixa
  passar um atalho apontando para fora da pasta: o caminho não tem `..` nenhum
  e ainda assim escapa. Só resolver o caminho de verdade pega os dois casos.
- `sem_barramento_o_agente_nao_consegue_gravar` (ao vivo) — se este passar a
  criar o arquivo, a aprovação virou enfeite.
- `as_ferramentas_que_gravam_pedem_card_com_o_nome_completo` — se
  `escrever_nota` escapasse do matcher do hook, o barramento seria uma porta
  dos fundos para exatamente o que o card existe para impedir.
- `no_sem_cabo_simplesmente_nao_existe` — a frase precisa ser a **mesma** para
  o nó desligado e para o inexistente. Duas mensagens diferentes fazem de cada
  tentativa uma sonda que mapeia o canvas inteiro.
- `dois_nos_esperando_um_pelo_outro_nao_travam_o_app` — verificado ao contrário:
  com a detecção de ciclo desligada, os dois nós travam e o teste falha. Um
  teste de "não trava" que nunca foi visto falhando não prova nada.
- `a_pasta_de_trabalho_e_um_lugar_so` — se dois lugares responderem "onde
  escrevo", uma sessão viva grava no worktree errado **com aprovação legítima**:
  o card diz a verdade sobre o conteúdo e mente sobre o destino.
- `o_trabalho_feito_a_mao_nao_vira_lixo_de_merge` — medido: mesclar numa pasta
  com alteração não gravada não é recusado pelo git; ele deixa marcador de
  conflito dentro do documento da pessoa.
- `a_chave_do_servidor_externo_nao_atravessa_a_fronteira_ipc` — a primeira
  tentativa (`skip_serializing` no campo) escondia o segredo do IPC **e do
  banco**, e o adaptador ficava sem o que mandar. Esconder é trabalho da borda,
  não do tipo.
- "não usa uma palavra de Git" (fumaça) — a tela de publicar é varrida atrás de
  `git`, `branch`, `commit`, `merge`, `worktree`, `rebase` e `stash`. A
  `Decisão 3` é promessa de produto, e promessa sem teste vira lembrete.

Um teste que passou merece um comentário no código quando o motivo dele não é
óbvio. Dois exemplos já no repositório: o `overflow` do nó, que tornava a porta
de ligação inclicável, e os listeners recriados a cada frame.

---

## 9. Decisões revistas em relação ao ARQUITETURA.md

**Canvas: SVG + DOM agora, WebGL depois.** O plano dizia PixiJS. O M0 usa SVG
para os cabos e DOM para os nós, sem biblioteca de canvas. Motivo: com quatro a
vinte nós isso é fluido, e uma dependência a menos é uma dependência a menos.

O gatilho para reavaliar é medido, não sentido: quando o quadro cair abaixo de
50 fps com nós em movimento no hardware alvo, ou passar de ~40 nós visíveis. Aí
a troca é local — `Cabos.tsx` e a camada de grade viram cena WebGL; os nós
continuam em DOM, porque texto selecionável e acessível não se desenha na GPU.

**Gravação por gesto, não por frame.** Arrastar não grava a cada movimento; o
banco recebe a posição final no `pointerup`. O viewport usa debounce de 400 ms.

**Trait do adaptador mais estreito que o esboço.** O `ARQUITETURA.md §5` previa
`iniciar`, `enviar`, `cancelar`, `retomar` e `eventos`. O implementado tem dois
métodos: `turno(texto) -> Receiver<EventoAgente>` e `cancelar()`.

- `enviar` e `eventos` viraram um só porque um turno é sempre pergunta e fluxo
  de resposta; separados, só sobrava a chance de chamar na ordem errada.
- `iniciar` e `retomar` saíram do trait e viraram trabalho da `Fabrica`, que
  recebe o `sessao_externa_id` no contexto e decide entre começar e continuar.
  Quem cria a sessão não deveria ser quem a conduz.
- `Stream` virou `std::sync::mpsc::Receiver`. O núcleo é crate puro e não tem
  runtime assíncrono; um canal da biblioteca padrão com uma thread por turno
  faz o mesmo sem arrastar tokio para dentro do modelo de domínio.

**Custo em dólar, não em real.** A maquete do §7 mostrava "R$ 0,42". A API cobra
em dólar, e converter exige uma cotação — cotação chumbada envelhece mal e
cotação buscada é serviço externo, que é decisão de produto. Até haver de onde
buscá-la, a interface mostra `US$`. Modelo sem preço na tabela mostra `—`,
nunca zero: zero mentiria e sumiria do painel.

**Um turno por vez, sem fila ainda.** O `ARQUITETURA.md §5` fala em fila por nó.
No M1 quem tenta falar durante um turno leva recusa com mensagem clara, e a
interface trava o campo. A fila em ordem é do M3, quando existir mais de um
remetente possível — antes disso ela não teria o que enfileirar.

### A que mais mexe no projeto: sem sidecar Node

O `ARQUITETURA.md §9` escolheu "sidecar Node com o Agent SDK", com uma
justificativa de uma linha: *entrada em streaming e `canUseTool` não existem na
CLI pura*. Com a CLI 2.1.251 na mão, isso não se sustenta:

- `--input-format stream-json` existe e está documentado no `--help`.
- A aprovação de ferramenta sai por `--permission-prompt-tool`, apontando para
  o servidor MCP do próprio app — que a §4 **já projeta**, com token e escopo
  por nó. A via CLI é mais alinhada ao desenho existente, não menos.

Sem a justificativa, sobra o custo: um runtime Node dentro do instalador do
Windows, uma árvore de `node_modules` para manter e mais um processo entre o
núcleo e o agente. O Rust faz o mesmo com `Command::spawn`.

**Se você discordar, o estrago é pequeno de propósito:** trocar é reescrever
`nucleo/src/claude.rs` e a linha da fábrica em `src-tauri/src/main.rs`. O trait
existe justamente para essa decisão continuar reversível.

### O custo vem da CLI, não da nossa tabela

`preco_por_milhao` em `modelo.rs` continua existindo, mas **o adaptador Claude
não a usa**: ele lê `total_cost_usd` do evento `result`.

Não é preguiça, é aritmética. Medido num turno real:

| | Tokens | |
|---|---|---|
| entrada nova | 6 | preço cheio |
| gravação de cache | 6 071 | 1,25× |
| leitura de cache | 108 511 | 0,1× |
| saída | 263 | preço cheio |

A CLI cobrou **US$ 0,0496**. A nossa tabela, que não sabe de cache, diria
**US$ 0,58** — quase 12 vezes mais. Um painel de custo com esse erro é pior que
painel nenhum, porque some a confiança em todos os outros números da tela.

A tabela segue valendo para adaptador que não reporta custo. Hoje, o falso.

### `--permission-prompt-tool` não existe mais

O `ARQUITETURA.md` e o plano do M2 que este arquivo trazia diziam que a
aprovação sairia por `--permission-prompt-tool` apontando para o servidor MCP
do app. **Essa flag não existe na CLI 2.1.251** — não está no `--help`, e
`grep -c permission-prompt` devolve zero.

O que existe, e é melhor, é um **hook `PreToolUse` do tipo `http`**: antes de
rodar uma ferramenta, a CLI faz POST do pedido para uma URL e a resposta
decide. Medido antes de projetar em cima:

| O que eu precisava saber | O que a medição mostrou |
|---|---|
| O hook recebe o quê? | `tool_name`, `tool_input` (com o conteúdo inteiro que seria gravado), `tool_use_id`, `cwd`, `session_id` |
| O cabeçalho com o token chega? | Chega intacto |
| Negar impede mesmo? | Impede — o arquivo não foi criado |
| A CLI espera a resposta? | Espera. Segurar oito segundos fez o agente esperar oito segundos |

A última linha é a que sustenta o marco. Sem ela o card seria teatro: o
arquivo seria gravado e desfeito. Com ela, ele não chega a ser gravado.

O formato da resposta:

```json
{ "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "allow" | "deny",
    "permissionDecisionReason": "…" } }
```

`permissionDecisionReason` vai para o **modelo**, não para a tela. Por isso o
texto de recusa diz "não tente de novo por outro caminho": sem isso, um agente
prestativo tenta gravar por outra ferramenta. Medido ao vivo — a resposta dele
depois de um "não" foi *"não vou tentar contornar o bloqueio por outro
caminho"*.

### Escrever passa só pelo hook

`Write` e `Edit` **não** estão no `--allowedTools`. A única porta para gravar é
o hook: se ele deixar de disparar por qualquer motivo — versão nova da CLI,
arquivo de settings corrompido, barramento fora do ar — a gravação é recusada
em vez de passar batida. O teste ao vivo
`sem_barramento_o_agente_nao_consegue_gravar` existe para essa trava não
enferrujar.

`--restricted` continua ligado, e `--tools` nomeia de volta o `Bash` que ele
tiraria: sem execução de código não há como montar um `.xlsx`, e cada uso de
`Bash` passa pelo card com o comando à vista.

### "Não perguntar de novo" não vale para tudo

A caixa aparece para `Write`, `Edit` e `NotebookEdit`. Para `Bash` e
`WebFetch`, não: uma licença permanente para rodar comando valeria pela máquina
inteira, concedida num clique que ninguém lembra uma semana depois. O card diz
"isto pergunta sempre" em vez de só esconder a caixa — o usuário precisa
entender que não é esquecimento.

A regra é por (workspace, ferramenta) — "gravar nesta pasta" —, nunca por
arquivo. Regra por arquivo vira uma lista que ninguém audita.

### Somente leitura até o M2

O M1 roda `--restricted` (tira Bash, execução de código e WebFetch, confina as
ferramentas de arquivo ao diretório de trabalho e **ignora as configurações do
usuário e do projeto**) mais um allowlist de `Read`, `Glob` e `Grep`.

Ignorar as configurações da máquina é o detalhe que mais importa: sem isso o
comportamento do agente dependeria do que houvesse em `~/.claude` de quem
instalou, e o mesmo workspace se comportaria diferente em cada computador.

Escrita chega quando existir o card de aprovação, não antes — `ARQUITETURA.md
§8` é explícito, e um agente que grava sem pedir licença é exatamente o que ele
proíbe.

### O que a CLI não conta pelo `result`

Nos dois erros que ela produziu de verdade — retomada de sessão inexistente e
estouro de `--max-turns` — o campo `result` **nem existe**, e a frase que o
usuário precisa ler sai pelo **stderr**:

```
No conversation found with session ID: 00000000-0000-0000-0000-000000000000
```

Por isso o adaptador **adia** o evento de erro sem texto até o stderr fechar, e
manda a última linha dele. Sem esse adiamento o usuário lê "o agente terminou
com erro (error_during_execution)", que não ajuda ninguém. É o que o teste
`retomada_de_sessao_que_nao_existe_diz_o_que_houve` protege.

Outros dois detalhes medidos, não supostos:

- **`stdin` precisa ser fechado.** Sem `Stdio::null()`, a CLI espera 3 segundos
  por dados que nunca vêm — em todo turno.
- **Texto intermediário não entra no histórico.** O `result` traz só a resposta
  final; a narração do meio do caminho é transmitida ao vivo pelos deltas e
  depois substituída. Os cards de ação contam o que aconteceu no intervalo.

---

## 10. Convenções

- **Código em português.** Nomes de função, variável, tabela e comando. O
  domínio é falado em português pelo usuário e por quem escreve; traduzir na
  fronteira só cria duas verdades.
- **Erro carrega o que a interface precisa mostrar.** Nada de `Result<_, String>`.
- **Nada de `unwrap()` fora de teste.** No núcleo, `Resultado<T>`.
- **Comentário explica o porquê, nunca o quê.** Se o código precisa de comentário
  para dizer o que faz, reescreva o código.
- **Um arquivo, um assunto.** `ipc.ts` é o único que fala com o backend;
  `db.rs` é o único que fala SQL.

---

## 11. Decisões tomadas

As quatro que estavam abertas foram fechadas em 31/08/2026. O que cada uma
manda fazer — e o que ela adia.

### 1. Nome: fica "Mutirão" por enquanto

Segue como codinome, sem decisão de marca. O custo de trocar é conhecido e
permanece congelado: `app.mutirao.desktop` no `tauri.conf.json`, a pasta
`%APPDATA%\app.mutirao.desktop`, o nome do binário e o do servidor MCP.

**Consequência:** enquanto o uso for interno, trocar o nome é renomear
constantes e mover uma pasta. Isso só encarece quando existir base instalada
fora da casa — ou seja, no dia da divulgação, não antes. **Reabrir esta decisão
antes de distribuir para fora**, que é o último momento barato.

### 2. Licença: proprietária, todos os direitos reservados

`LICENSE` na raiz, `"license": "UNLICENSED"` no `package.json`,
`license-file` e `publish = false` nos dois `Cargo.toml` — o `publish = false`
existe para que um `cargo publish` distraído não empurre o núcleo para o
crates.io.

**Consequência:** repositório privado. E vale dizer o que privado *não* é —
não é cofre. Quem tem acesso de leitura leva tudo que estiver commitado, e
histórico do Git não esquece. Daí a regra da chave abaixo.

### 3. Chave de API: a do dono, pelo ambiente, nunca no repositório

Não existe backend de cobrança, não existe revenda de crédito, não existe conta
de usuário. Uma chave só, a de quem roda o app.

O adaptador Claude (M1) roda num sidecar Node. A chave chega nele por variável
de ambiente do processo — `ANTHROPIC_API_KEY` — e o Mutirão nunca a escreve em
disco, nem no SQLite, nem em log. Para o M1 basta a variável já estar no
ambiente de quem abre o app; o `.gitignore` cobre `.env*` para que um arquivo
local de conveniência não vaze por descuido.

> A ordem exata de resolução de credencial do Agent SDK (variável de ambiente
> versus perfil em disco) deve ser conferida na documentação do SDK ao escrever
> o adaptador, não deduzida daqui.

**Consequência no produto:** o M6 perde o backend de cobrança inteiro, e o
onboarding deixa de ter tela de login — vira "detectou a CLI, achou a chave,
pronto". Quando houver usuário fora da casa, esta decisão volta à mesa: chave
própria de cada um é o caminho barato; revenda exige servidor, e servidor muda
a arquitetura, não só o M6.

**O que não muda por ser interno:** o teto de custo por workspace e o orçamento
por trace continuam sendo do M6. Uma chave só, sem limite, é exatamente a
configuração em que um ciclo A→B→A malcomportado queima crédito real. O risco
"custo de tokens descontrolado" do `ARQUITETURA.md §11` fica *mais* relevante,
não menos.

### 4. Cobrança: nenhuma, por enquanto — uso interno

Sem preço, sem licenciamento, sem telemetria. A divulgação é uma decisão futura,
tomada quando a ferramenta estiver boa, e é ela que reabre os itens 1, 3 e 4
juntos.

**Consequência:** o critério de pronto do M6 muda. Era *"alguém que não é
programador instala, conecta e faz um trabalho útil sem me chamar"*. Enquanto
for interno, é *"eu instalo numa máquina limpa e trabalho, sem montar ambiente
de desenvolvimento"*. Instalador e auto-update continuam valendo — reinstalar na
mão a cada correção é imposto diário. Assinatura de código passa a ser
opcional: sem ela o SmartScreen reclama uma vez por máquina, o que é irritante
para três máquinas e inviável para trezentas.

---

## 11b. O que continua aberto

Nada que trave o M1. Ficam para o dia da divulgação:

- **Marca definitiva** e o custo de renomear com base instalada.
- **Se a chave passa a ser de cada usuário** ou revendida — e, se revendida, o
  backend que isso implica.
- **Modelo de cobrança**, que só existe depois que houver o que cobrar.

---

## 12. Onde parar e olhar

Ao terminar cada marco, a pergunta não é "implementei?" e sim o critério de
pronto do `ARQUITETURA.md`.

- **M0** — *arrasto três caixas, fecho o app, reabro e está tudo no lugar.*
  Funciona.
- **M1** — *peço "resuma este PDF" e vejo a resposta chegando em bolhas, com o
  custo ao lado.* Funciona, contra o Claude Code de verdade.
- **M2** — *o agente monta um arquivo na minha pasta e eu aprovo a gravação
  antes de acontecer.* Funciona. Medido em `nucleo/tests/ao_vivo.rs`: o card
  aparece, o arquivo **não existe** enquanto ele está aberto, e passa a existir
  depois do clique.

- **M3** — *o Pesquisador entrega ao Redator sem eu tocar, e um ciclo A→B→A
  encerra sozinho sem travar o app.* Funciona. Medido com **dois** processos do
  Claude Code de verdade em `o_pesquisador_entrega_ao_redator_sem_eu_tocar`: o
  recado atravessa em 17 segundos, o Redator responde, a resposta volta na
  mesma cadeia, e a cadeia inteira custou US$ 0,088.
- **M4** — *um prompt monta um time de quatro, e amanhã eu reabro o mesmo time
  como estava.* Funciona. Em `um_prompt_monta_um_time`, o Organizador montou
  Chefe, Ana (Pesquisador), Bruno e Carla (Redator), cada um com papel, cabo e
  trabalho recebido. E `o_papel_muda_o_que_o_agente_faz_de_verdade` prova a
  outra metade do que um papel vale: o Pesquisador, mandado gravar um arquivo,
  leu, **não gravou** e explicou que ia delegar — porque o papel `cauteloso`
  não recebe ferramenta de escrita nenhuma.
- **M5** — *dois ensaios do mesmo trabalho rodam ao mesmo tempo e eu publico um
  deles sem entender de Git.* Funciona. Com o Claude Code de verdade em
  `dois_agentes_trabalham_em_dois_rascunhos_do_mesmo_trabalho`: dois rascunhos
  guardaram 24 e 12 meses ao mesmo tempo, a pasta de verdade ficou nos 18
  originais, publicar um levou o 24 e o outro continuou com o 12 — e a pasta do
  usuário não tem nada oculto dentro.

### O que ficou de fora do M3, M4 e M5, com intenção

1. **Adaptador Codex.** O plano previa fazê-lo no M3 "para provar que a ponte é
   agnóstica". A ponte é agnóstica por construção — quem fala com outro nó é o
   `Orquestrador`, e o adaptador só emite `EventoAgente` —, mas isso é
   argumento, não prova, e a prova exige a CLI do Codex instalada. Fica para
   quando ela estiver na máquina.
2. **Editor de papéis na interface.** O núcleo tem `criar_papel`,
   `editar_papel` e `remover_papel`, e o IPC os expõe; a interface só **usa** a
   biblioteca, não a edita. Um editor é uma tela, não uma decisão de
   arquitetura — e a biblioteca embutida cobre o trabalho de hoje.
3. **Face terminal com histórico.** Ela mostra o fluxo cru a partir do turno
   seguinte, porque o fluxo não é gravado — só o que ele produz.
4. **O portal de navegador (WebView2 + CDP).** É o terceiro item do M5 e o
   único que não entregamos. É Windows-only por construção — WebView2 não
   existe em Linux, que é onde este código foi escrito —, então não dá para
   escrever nem verificar aqui. O nó `portal` continua maquete e diz isso na
   tela. Fica junto do adaptador Codex, pela mesma razão: o item pede prova, e
   prova exige a plataforma.
5. **Interface para servidores MCP externos.** O núcleo e o IPC têm
   `definir_mcp_do_papel`; a tela para digitar endereço e chave não existe
   ainda. É a mesma situação do editor de papéis — uma tela, não uma decisão.
6. **Escolher a pasta do workspace pela interface.** Ela nasce em
   `Documentos/Mutirão/<nome>`. Um seletor de pasta exige o plugin de diálogo
   do Tauri; é uma tela, não uma decisão de arquitetura.

### Começando o M6

O M6 é *"eu instalo numa máquina limpa e trabalho, sem montar ambiente de
desenvolvimento"* — o critério já ajustado pela Decisão 4 do §11, porque
enquanto o uso for interno não existe "alguém que não é programador". O que o
M5 deixa pronto:

1. O app roda inteiro sem nada além da CLI do Claude Code e do Git, e **diz na
   barra** quando falta um dos dois. Metade do onboarding é isso: detectar e
   contar, em vez de quebrar.
2. `AdaptadorClaude::detectar` já é a sonda que o onboarding precisa, e
   `MUTIRAO_CLAUDE_BIN` já cobre a CLI fora do PATH — comum no Windows, onde
   ela vira um `.cmd` numa pasta do npm.
3. As migrations já sobem sozinhas e são idempotentes, então atualizar o app
   sobre um banco antigo é caminho testado desde a 002.

O que precisa nascer: instalador MSI/NSIS com auto-update, a tela de primeira
abertura, e a escolha da pasta do workspace por diálogo do sistema.

Três cuidados que os marcos anteriores compraram caro:

- **Assinatura de código é opcional, não gratuita.** Sem ela o SmartScreen
  reclama uma vez por máquina. Irritante para três máquinas, inviável para
  trezentas — e a Decisão 4 diz que hoje são três.
- **O auto-update mexe no app enquanto ele pode estar com agente rodando.**
  Vale o mesmo padrão do `ensaios::trocar`: derrubar os processos antes, nunca
  depois.
- **A chave da API nunca entra no instalador.** Ela é lida do ambiente, e a
  tela de primeira abertura tem de dizer isso em vez de pedir para colar.

Escreva o roteiro novo para o adaptador falso no mesmo dia — é ele que mantém
o custo de cada iteração em zero.
