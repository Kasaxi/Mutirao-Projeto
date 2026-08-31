# Mutirão — Especificação de implementação

Companheiro do `ARQUITETURA.md`. Aquele diz **o quê** e **por quê**; este diz
**onde** e **como**, com contratos exatos. Um agente de código deve conseguir
abrir este arquivo e escrever a próxima função sem inventar nome, caminho ou
formato.

Estado atual: **M0 pronto e testado.** M1 em diante ainda é contrato, não código.

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
│   │   └── 001_inicial.sql esquema completo, executável
│   └── src/
│       ├── lib.rs          fachada + 23 testes
│       ├── modelo.rs       tipos de domínio e máquina de estados
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
│       └── Cabos.tsx       SVG dos cabos
│
└── testes-ui/
    ├── fumaca.mjs          10 verificações no Chromium
    └── canvas.png          retrato gerado pelo teste
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

cargo test -p nucleo        # 23 testes do núcleo
node testes-ui/fumaca.mjs   # teste de fumaça da interface
```

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

`abrir_workspace` devolve o estado inteiro numa viagem só. Abrir um canvas não
pode custar três chamadas de IPC.

### Eventos Rust → front (M1 em diante)

Ainda não emitidos. Nomes reservados, payload definido:

| Evento | Payload | Quando |
|---|---|---|
| `sessao:evento` | `{ sessionId, evento: EventoAgente }` | a cada evento do adaptador |
| `sessao:estado` | `{ sessionId, estado, pedeAtencao }` | mudança na máquina de estados |
| `aprovacao:pedida` | `{ toolCallId, sessionId, ferramenta, argumentos }` | ferramenta exige aprovação |
| `aprovacao:decidida` | `{ toolCallId, aprovada }` | usuário decidiu |
| `no:mensagem` | `{ deNode, paraNode, traceId }` | mensagem entre nós, para animar o cabo |
| `custo:atualizado` | `{ workspaceId, total, porNo }` | fim de turno |

Regra: evento **notifica**, não carrega o histórico. O front pede o que
precisa por comando. Isso evita que a fronteira IPC vire um firehose.

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
| Núcleo | `cargo test -p nucleo` — 23 testes | migrations, CRUD, escopo dos cabos, validação, máquina de estados, contrato de serialização |
| Interface | `node testes-ui/fumaca.mjs` — 10 verificações no Chromium | pan, zoom ancorado, arrastar, redimensionar, ligar, renomear, remover, console limpo |
| Agentes (M1+) | adaptador falso, roteirizado | fluxo de turno sem gastar token |

O adaptador falso do M1 é obrigatório, não conveniência: testar orquestração
contra a API de verdade é lento, caro e não-determinístico. Ele lê um roteiro
(`{ atraso_ms, eventos: [...] }`) e emite os mesmos `EventoAgente`.

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

## 11. Decisões ainda abertas

Estas mudam código e nenhuma delas é minha para tomar:

1. **Nome do produto.** "Mutirão" é codinome. Trocar depois custa: identificador
   do app (`app.mutirao.desktop`), pasta de dados, nome do binário e do MCP.
   Decidir antes do M6 é barato; depois, não.
2. **Licença.** Aberto atrai contribuição e cópia; fechado protege pouco num
   nicho onde o concorrente principal é open source. A escolha muda desde o
   README até a estratégia de distribuição.
3. **Chave de API.** O usuário traz a dele (simples, sem risco de crédito) ou
   você revende (melhor experiência, exige backend de cobrança e limite). Isso
   define se existe servidor no produto ou não.
4. **Modelo de cobrança.** Pagamento único como o Maestri, assinatura, ou grátis
   com pago para times. Muda o M6 inteiro.

---

## 12. Onde parar e olhar

Ao terminar cada marco, a pergunta não é "implementei?" e sim o critério de
pronto do `ARQUITETURA.md`. O do M0 era: *arrasto três caixas, fecho o app,
reabro e está tudo no lugar.* Isso funciona.

O próximo — M1 — é: *peço "resuma este PDF" e vejo a resposta chegando em
bolhas, com o custo ao lado.* Comece pelo adaptador Claude e pelo adaptador
falso no mesmo dia; sem o falso, cada iteração custa dinheiro e paciência.
