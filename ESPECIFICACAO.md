# Mutirão — Especificação de implementação

Companheiro do `ARQUITETURA.md`. Aquele diz **o quê** e **por quê**; este diz
**onde** e **como**, com contratos exatos. Um agente de código deve conseguir
abrir este arquivo e escrever a próxima função sem inventar nome, caminho ou
formato.

Estado atual: **M0 pronto e testado. M1 quase todo pronto** — sessão, turno,
custo, face conversa e face terminal funcionam ponta a ponta contra o adaptador
falso. Falta o adaptador Claude, e é ele que separa "conversa com roteiro" de
"conversa com modelo". M2 em diante ainda é contrato, não código.

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
│   │   └── 002_adaptador_falso.sql  'falso' no CHECK de session.adaptador
│   └── src/
│       ├── lib.rs          fachada + 43 testes
│       ├── modelo.rs       tipos de domínio, máquina de estados, preços
│       ├── agente.rs       trait AgenteAdapter, Roteiro, adaptador falso
│       ├── orquestrador.rs turno, bomba de eventos, custo
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
│       └── Cabos.tsx       SVG dos cabos
│
└── testes-ui/
    ├── fumaca.mjs          27 verificações no Chromium
    ├── canvas.png          retrato do canvas em repouso
    └── conversa.png        retrato de um turno inteiro
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
| Git for Windows (opcional) | usado pela ferramenta Bash do Claude Code, no M1 |

### Comandos

```bash
npm install                 # dependências do front
npm run dev                 # front sozinho, no navegador, com núcleo falso
npm run app                 # app de verdade (tauri dev)
npm run build               # typecheck + build do front
npm run app:build           # instalador MSI/NSIS

cargo test -p nucleo        # 43 testes do núcleo
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
| `abrir_sessao` | `nodeId, adaptador` | `Sessao` | `nao_encontrado`, `invalido` (nó não é agente) |
| `sessao_do_no` | `nodeId` | `Sessao \| null` | — |
| `enviar_mensagem` | `sessionId, texto` | — | `invalido` (vazia, turno em andamento), `nao_encontrado` |
| `cancelar_turno` | `sessionId` | — | `nao_encontrado` |
| `historico` | `sessionId, limite` | `Mensagem[]` | — |
| `acoes_da_sessao` | `sessionId` | `ChamadaFerramenta[]` | — |
| `custo_do_workspace` | `workspaceId` | `{ total, por_no }` | — |

`abrir_workspace` devolve o estado inteiro numa viagem só. Abrir um canvas não
pode custar três chamadas de IPC.

`abrir_sessao` devolve a sessão que já existir naquele nó. Reabrir o app
continua a conversa; não começa outra.

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
| `aprovacao:pedida` | `{ tool_call_id, session_id, ferramenta, argumentos }` | ferramenta exige aprovação | M2 |
| `aprovacao:decidida` | `{ tool_call_id, aprovada }` | usuário decidiu | M2 |
| `no:mensagem` | `{ de_node, para_node, trace_id }` | mensagem entre nós, para animar o cabo | M3 |

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
hex), grava em `session.token` e o injeta na configuração MCP daquele agente:

```json
{
  "mcpServers": {
    "mutirao": {
      "type": "http",
      "url": "http://127.0.0.1:{porta}/mcp",
      "headers": { "X-Mutirao-Token": "{token}" }
    }
  }
}
```

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

listar_arquivos { caminho?: string } -> { itens: { nome, tipo, tamanho }[] }
ler_arquivo     { caminho: string } -> { conteudo: string, truncado: boolean }
escrever_arquivo { caminho: string, conteudo: string } -> { bytes: number }

recrutar { papel: string, nome: string } -> { no: string }
dispensar { no: string } -> { encerrado: true }

perguntar_humano { pergunta: string, opcoes?: string[] } -> { resposta: string }
concluir { resumo: string } -> { ok: true }
```

O agente endereça vizinhos **por nome**, não por id: id é detalhe interno e
convida a erro de cópia. A resolução nome → id acontece dentro do conjunto
visível, e nome ambíguo é erro explícito.

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

Limites do host, não do agente: **6 saltos**, prazo por mensagem, orçamento de
tokens por `trace`. Estourou qualquer um: a cadeia encerra, quem pediu recebe
erro, e o usuário vê um aviso — nunca um silêncio.

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
| Núcleo | `cargo test -p nucleo` — 43 testes | migrations, CRUD, escopo dos cabos, validação, máquina de estados, contrato de serialização, turno inteiro, custo, cancelamento, sigilo do token |
| Interface | `node testes-ui/fumaca.mjs` — 27 verificações no Chromium | pan, zoom ancorado, arrastar, redimensionar, ligar, renomear, remover; e um turno de ponta a ponta: pergunta, card de ação, resposta, custo, face terminal, parar |

O adaptador falso é obrigatório, não conveniência: testar orquestração contra a
API de verdade é lento, caro e não-determinístico. Ele lê um roteiro
(`{ atraso_ms, eventos: [...] }`) e emite os mesmos `EventoAgente`. Existe em
duas encarnações — `nucleo/src/agente.rs` para o app e os testes do núcleo, e
um espelho em `src/lib/ipc.ts` para o modo navegador, que não tem Rust por
baixo. A duplicação é conhecida e vale o preço: sem ela não dá para desenvolver
nem testar a interface fora do Tauri.

Três testes valem por si, porque cobrem coisa que falha calada:

- `o_token_do_mcp_nunca_sai_no_json_da_sessao` — o segredo do §4 atravessando a
  fronteira seria a falha de segurança mais barata de cometer e a mais difícil
  de notar.
- `adaptador_que_cala_no_meio_deixa_o_no_em_erro_e_nao_pensando` — nó preso em
  "pensando" não pede atenção, não aceita turno novo e não explica nada.
- `migration_002_reconstroi_session_sem_perder_os_filhos` — reconstruir tabela
  com FK ligada apaga os filhos por CASCADE sem erro nenhum.

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
  custo ao lado.* Funciona **contra o adaptador falso**, e o teste de fumaça
  mede exatamente isso. Contra um modelo de verdade, ainda não: falta o
  adaptador Claude.

### O que falta no M1

1. **Adaptador Claude.** Um `AgenteAdapter` que sobe o Agent SDK num sidecar
   Node e traduz a saída para `EventoAgente`. Tudo à volta já existe: a
   `Fabrica` é o único lugar que precisa mudar, e o `ContextoSessao` já carrega
   pasta, token do MCP e `sessao_externa_id`. Confira a ordem de resolução de
   credencial na documentação do SDK ao escrever — não deduza.
2. **Retomada de verdade.** A conversa já sobrevive ao fechamento: fica no
   SQLite e volta ao reabrir o nó, e `sessao_externa_id` é gravado. Retomar a
   sessão *do agente* depende do adaptador — é `--resume` no Claude Code, e só
   dá para provar com ele no lugar.
3. **Face terminal com histórico.** Hoje ela mostra o fluxo cru a partir do
   turno seguinte, porque o fluxo não é gravado — só o que ele produz. Se valer
   guardar, é uma tabela nova, e aí é decisão, não esquecimento.

Ao escrever o adaptador Claude, escreva um roteiro novo para o falso no mesmo
dia. É ele que mantém o custo de cada iteração em zero.
