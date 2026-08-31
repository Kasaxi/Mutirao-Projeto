# Mutirão — Plano técnico v1

> Orquestrador de agentes de IA em canvas infinito, para trabalho geral (não só código).
> Windows 11 x64. "Mutirão" é codinome de trabalho.
> Documento base para implementação — 31/08/2026.

---

## 1. O que muda em relação ao Maestri

A camada visual e de orquestração é idêntica em ambição: canvas infinito, agentes como nós,
cabos entre eles, notas compartilhadas, papéis, maestro que recruta, trabalho paralelo,
layouts salvos. Coordenação serve para escrever um contrato tanto quanto para escrever código.

O que muda é o substrato. O Maestri assume repositório Git, terminal como interface, e um
usuário que sabe o que é branch. Trocar essas três premissas é o produto inteiro.

| Dimensão | Maestri | Mutirão |
|---|---|---|
| Unidade de trabalho | Repositório Git | Pasta de trabalho, arquivos de qualquer tipo |
| Cara do nó | Terminal | Conversa por padrão; terminal a um clique |
| Como o agente roda | Processo interativo em PTY | Sessão headless com stream de eventos |
| Ponte entre agentes | CLI + skill; fim de turno detectado no terminal | Servidor MCP do app; agentes se falam por ferramentas |
| Trabalho paralelo | Floors sobre branch Git | Ensaios sobre Git oculto; usuário não vê "branch" |
| Ferramentas | Editar código, rodar comando, navegador | Arquivos, documentos, planilhas, navegador, e-mail, APIs — via MCP |
| Segurança | Confiança no dev | Aprovação explícita de ação externa e destrutiva |

---

## 2. As três decisões que definem o sistema

### Decisão 1 — O nó não é um terminal. É uma sessão com duas faces.

Rodar o agente em modo headless com saída estruturada, não como processo interativo num PTY.

- Claude Code: `-p --output-format stream-json`
- Codex: `codex exec --json` → JSONL com `thread.started`, `turn.started`, `item.*`, `turn.completed`

Isso resolve o ponto mais frágil do Maestri, que precisa *adivinhar* pelo texto do terminal
quando o agente terminou. Com stream estruturado, o fim do turno é evento explícito.

Consequência de produto:

- **Face conversa** (padrão): bolhas, cards de ação ("editou `orçamento.xlsx`"), botões de aprovar
- **Face terminal**: a mesma sessão em modo cru, para quem quiser

Sem essa dupla face não existe produto para público geral.

### Decisão 2 — A orquestração é um servidor MCP, não uma CLI.

O app sobe um servidor MCP local e anexa a cada sessão (Claude: `--mcp-config`; Codex tem
equivalente). Falar com outro agente vira uma **ferramenta**, do mesmo tipo que ler um arquivo.

Ganhos: funciona com qualquer agente que fale MCP; chamada estruturada e validada; resposta
volta pelo mesmo canal; e o mesmo servidor entrega notas, arquivos, recrutamento e
pergunta-ao-humano sem inventar protocolo novo.

### Decisão 3 — Git existe, mas o usuário nunca fica sabendo.

Todo workspace é uma pasta comum. Na criação, o app inicializa um repositório Git **oculto**
(`.mutirao/`, atributo hidden no Windows) com aquela pasta como worktree. O usuário vê
"Rascunho 2" e "Publicar" — nunca branch, commit ou merge.

Dá de graça: histórico com desfazer, isolamento paralelo (`git worktree` por ensaio), detecção
de conflito. Para binários (`.docx`, `.xlsx`, imagem) não se tenta merge: mostra as duas
versões e o usuário escolhe.

---

## 3. Arquitetura em camadas

```
Interface    Canvas híbrido — cena WebGL (grade, cabos) + nós em DOM absoluto
                sob a mesma matriz de transformação
   ▽
Núcleo       Orquestrador em Rust — estado, fila de turnos por nó, roteamento,
                aprovações pendentes, log de auditoria. Único que escreve no banco
   ▽
Adaptadores  Um por agente — inicia, alimenta, retoma, cancela; traduz a saída
                para o mesmo fluxo de eventos interno
   ▽
Barramento   Servidor MCP do Mutirão — anexado a toda sessão. Ferramentas de
                orquestração + ferramentas de trabalho com escopo
   ▽
Substrato    Pasta + SQLite + Git oculto
```

---

## 4. Modelo de domínio (SQLite)

```sql
-- espaço de trabalho: uma pasta no disco
workspace(id, nome, pasta, criado_em, ensaio_ativo)

-- tudo que existe no canvas; tipo define o payload
node(id, workspace_id, ensaio_id, tipo, nome, x, y, w, h, config_json)
     -- tipo ∈ agente | nota | arquivos | portal | forma

-- cabos: quem pode falar com quem, quem lê o quê
edge(id, workspace_id, de_node, para_node, tipo)
     -- tipo ∈ fala_com | le_nota | escreve_nota

-- uma sessão viva de agente, retomável
session(id, node_id, adaptador, sessao_externa_id, estado, pid, custo_total)
     -- estado ∈ ocioso | pensando | aguardando_aprovacao | aguardando_humano | erro

-- histórico da conversa; alimenta a face conversa
message(id, session_id, papel, conteudo, tokens, custo, criado_em, trace_id)

-- cada ação do agente vira linha; é o log de auditoria
tool_call(id, session_id, ferramenta, argumentos_json, resultado_json,
          aprovacao, decidido_por, criado_em)
     -- aprovacao ∈ automatica | pendente | aprovada | negada

-- papel = prompt de sistema + conjunto de ferramentas + autonomia
role(id, nome, prompt, ferramentas_json, nivel_autonomia)

-- ensaio = worktree git isolado; o "floor" para leigos
ensaio(id, workspace_id, nome, branch, caminho_worktree, base_commit, estado)

-- partitura = time inteiro salvo para reabrir amanhã
partitura(id, workspace_id, nome, snapshot_json)
```

---

## 5. Runtime de agente

> Esboço original. O implementado é mais estreito — dois métodos em vez de
> cinco, e `Receiver` em vez de `Stream`. Ver `nucleo/src/agente.rs` e a
> justificativa em `ESPECIFICACAO.md §9`.

```rust
trait AgenteAdapter {
    fn iniciar(pasta, papel, mcp_config) -> SessionId;
    fn enviar(SessionId, texto);                    // novo turno
    fn cancelar(SessionId);                         // interrompe o turno atual
    fn retomar(sessao_externa_id) -> SessionId;     // depois de fechar o app
    fn eventos(SessionId) -> Stream<EventoAgente>;
}

enum EventoAgente {
    SessaoIniciada      { id_externo, modelo, ferramentas },
    TextoParcial        { delta },
    Raciocinando        { resumo },
    FerramentaPedida    { id, nome, argumentos },      // pode exigir aprovação
    FerramentaConcluida { id, resultado, erro },
    TurnoConcluido      { texto_final, tokens, custo }, // o "sino"
    PrecisaHumano       { pergunta },
    Erro                { mensagem, recuperavel },
}
```

| Adaptador | Como roda | Retomada | Aprovação de ferramenta |
|---|---|---|---|
| **Claude** | Preferir o Agent SDK (TypeScript) num sidecar Node — dá entrada em streaming na mesma sessão e callback `canUseTool`. Alternativa simples: `claude -p --output-format stream-json` | `--resume <session-id>`; sessões persistem em disco | `canUseTool` no SDK; na via CLI, `--permission-prompt-tool` apontando para o servidor MCP do app + `--allowedTools` |
| **Codex** | `codex exec --json` (JSONL na stdout, `turn.completed`); `--output-schema` para resposta estruturada | `codex exec resume <id>` ou `--last` | `--sandbox workspace-write` como padrão; nunca acesso total sem pedir |
| **PTY genérico** | `portable-pty` (Rust) sobre ConPTY, saída crua para xterm.js | Não há | Nenhuma. Só face terminal, modo avançado |

**Regra do orquestrador:** um turno por vez por nó. Mensagens que chegam durante um turno
entram na fila do nó, em ordem. Sem isso, dois agentes falando com o mesmo terceiro produzem
intercalação de contexto e resposta sem sentido.

---

## 6. Barramento: ferramentas de orquestração

Servidor MCP local anexado a toda sessão, com escopo por nó — cada agente só enxerga os nós
aos quais está ligado por um cabo.

| Ferramenta | O que faz | Bloqueia? |
|---|---|---|
| `enviar_para(no, mensagem, refs)` | Pergunta a outro nó e espera a resposta | Sim, com prazo |
| `avisar(no, mensagem)` | Entrega e segue em frente | Não |
| `ler_nota(nota)` / `escrever_nota` | Memória compartilhada em Markdown no disco | Não |
| `listar/ler/escrever arquivo` | Sistema de arquivos com escopo do workspace | Não |
| `recrutar(papel, nome)` | Cria nó novo já conectado — o Maestro Mode | Não |
| `dispensar(no)` | Encerra a sessão de um recrutado | Não |
| `perguntar_humano(pergunta)` | Card no canvas + notificação; agente pausa | Sim, sem prazo |
| `concluir(resumo)` | Marca a tarefa como entregue e aquieta o nó | Não |

### Protocolo de mensagem

```json
{
  "id": "msg_7f3a",
  "de": "no_pesquisa",
  "para": "no_redator",
  "tipo": "pedido",
  "corpo": "revise a seção de garantias do contrato",
  "refs": ["nota_briefing", "contrato_v3.docx"],
  "trace": "tr_91c2",
  "saltos": 2,
  "prazo_ms": 600000
}
```

Ciclo A→B→A é legítimo e comum. O que mata é o ciclo infinito: limite de **6 saltos**, prazo por
mensagem, e **orçamento de tokens por trace** que, ao estourar, encerra a cadeia e avisa o
usuário em vez de queimar crédito em silêncio.

---

## 7. Ferramentas de trabalho: MCP em vez de integrações

O agente de código precisa de duas ferramentas: editar arquivo e rodar comando. O agente geral
precisa de vinte — e escrever vinte integrações à mão é o que mata o projeto no terceiro mês.

A saída é não escrever nenhuma: **ser um host MCP**. Cada papel recebe um conjunto de servidores
MCP. O Windows 11 ajuda: o *On-Device Agent Registry* é um registro de servidores MCP no nível
do sistema, com contenção, controle por Configurações e auditoria.

Vale embutir mesmo assim:

- **Arquivos com escopo** — a pasta do workspace e nada além
- **Documentos** — ler e escrever `.docx`, `.xlsx`, `.pdf`
- **Portal de navegador** — WebView2 extra no canvas, controlada por CDP
- **HTTP** — chamada de API genérica, aprovação obrigatória em tudo que não for GET

---

## 8. Segurança e autonomia

Um agente geral roda na máquina de alguém que não entende o risco, e vai mexer em contrato,
planilha financeira e e-mail. Permissão é requisito de v1, não acessório.

**Três níveis por nó:**

- **Cauteloso** — aprovação para qualquer escrita. Padrão para papéis novos.
- **Padrão** — lê e escreve dentro da pasta sozinho; aprovação para tudo que sai da máquina ou apaga.
- **Solto** — livre dentro da pasta. Ação externa sempre pede aprovação, em qualquer nível.

Não existe modo "pula todas as permissões" na interface.

**Mecanismo:** o pedido de ferramenta chega ao núcleo antes de executar. Se exigir aprovação,
vira card no nó, turno em `aguardando_aprovacao`, notificação ao usuário. Toda decisão vai para
`tool_call` com quem decidiu e quando — log append-only, exportável.

Escopo de arquivos é verificado no núcleo com caminho canônico, não confiando no agente:
qualquer caminho que escape da pasta é negado antes de chegar ao disco.

---

## 9. Stack

| Camada | Escolha | Por quê |
|---|---|---|
| Shell do app | **Tauri 2** (Rust + WebView2) | ~10 MB vs ~150 MB de Electron; a mesma WebView2 serve de portal; núcleo Rust aguenta dezenas de sessões |
| Interface | React + TypeScript | Ecossistema; a face conversa é UI densa |
| Canvas | PixiJS v8 (WebGL) + nós em DOM | Cabos e grade na GPU; texto em DOM real. Híbrido é o que permite 20 nós vivos |
| Terminal | xterm.js com renderer WebGL | Padrão de fato; só na face terminal |
| PTY | `portable-pty` (Rust) → ConPTY | API nativa do Windows; evita Node no caminho crítico |
| Estado | SQLite (`rusqlite`) + arquivos | Metadados no banco, conteúdo em arquivo que abre no Explorer |
| Runtime Claude | Sidecar Node com o Agent SDK | Entrada em streaming e `canUseTool` não existem na CLI pura |
| Versionamento | Git oculto (`git2` ou git embarcado) | Histórico, ensaios e conflito de graça |
| Distribuição | MSI/NSIS + `tauri-plugin-updater` | Auto-update desde o dia um; reservar orçamento para assinatura de código (SmartScreen) |

---

## 10. Marcos

Cada marco tem critério de pronto verificável — não "implementado", mas "eu consigo fazer X".
Estimativa para um desenvolvedor com apoio pesado de agentes.

### M0 — Esqueleto · 1 semana
- Janela Tauri, canvas com pan e zoom, nós DOM arrastáveis e redimensionáveis
- SQLite com o esquema acima; layout persistido

**Pronto quando:** arrasto três caixas, fecho o app, reabro e está tudo no lugar.

### M1 — Um agente vivo · 2 semanas
- Adaptador Claude com eventos normalizados; face conversa e face terminal
- Cancelar turno, retomar sessão depois de fechar o app, custo por turno visível

**Pronto quando:** peço "resuma este PDF" e vejo a resposta chegando em bolhas, com o custo ao lado.

**Onde está:** o critério passa contra o adaptador falso, medido pelo teste de fumaça — sessão,
turno, cancelamento, cards de ação, custo por turno e por workspace, face conversa e face
terminal. Falta o adaptador Claude, e com ele a retomada da sessão externa. A lista exata do que
sobra está em `ESPECIFICACAO.md §12`.

### M2 — Substrato de trabalho · 2 semanas
- Pasta do workspace, nó de árvore de arquivos, notas Markdown editáveis no canvas
- Servidor MCP do app no ar com arquivos e notas; fluxo de aprovação com card

**Pronto quando:** o agente monta um `.xlsx` na minha pasta e eu aprovo a gravação antes de acontecer.

### M3 — A ponte · 3 semanas
- Cabos definindo quem fala com quem; ferramentas `enviar_para` e `avisar`
- Fila de um turno por nó, limite de saltos, prazo, orçamento por trace
- Adaptador Codex, para provar que a ponte é agnóstica

**Pronto quando:** Pesquisador entrega ao Redator sem eu tocar, e um ciclo A→B→A encerra sozinho
sem travar o app.

### M4 — Times · 2 semanas
- Papéis com prompt, ferramentas e nível de autonomia; biblioteca de papéis prontos
- Maestro Mode: `recrutar` e `dispensar`; partituras salvas e reabertas

**Pronto quando:** um prompt monta um time de quatro, e amanhã eu reabro o mesmo time como estava.

### M5 — Paralelo e mundo externo · 3 semanas
- Ensaios sobre worktree oculto, com tela de publicar em linguagem de gente
- Portal de navegador em WebView2 controlável por CDP
- Host MCP: anexar servidores externos por papel

**Pronto quando:** dois ensaios do mesmo trabalho rodam ao mesmo tempo e eu publico um deles
sem entender de Git.

### M6 — Ferramenta instalável · 2 semanas
- Instalador e auto-update — reinstalar na mão a cada correção é imposto diário
- Onboarding que detecta e instala as CLIs de agente e acha a chave no ambiente
- Painel de custo por workspace e por nó, com teto configurável

**Pronto quando:** instalo numa máquina limpa e trabalho, sem montar ambiente de desenvolvimento.

Encolheu porque o uso é interno (`ESPECIFICACAO.md §11`): sem backend de cobrança, sem tela de
login, sem conta de usuário. Assinatura de código vira opcional — sem ela o SmartScreen reclama
uma vez por máquina, tolerável em três, inviável em trezentas. O painel de custo **não** é
opcional: uma chave só, sem teto, é a configuração exata em que um ciclo malcomportado queima
crédito de verdade.

**Total: 14 a 15 semanas** até algo instalável. M0–M3 (8 semanas) já é ferramenta de uso diário —
ponto certo para parar e usar de verdade antes de investir o resto.

---

## 11. Riscos

| Risco | Sev. | Mitigação |
|---|---|---|
| Custo de tokens descontrolado | Alto | Custo visível por turno e por nó, teto por workspace, orçamento por trace, aviso na metade do teto |
| Dois agentes no mesmo arquivo | Alto | Lease por arquivo concedido pelo núcleo; para paralelo real, ensaios separados |
| Agente travado sem avisar | Médio | Prazo por turno, heartbeat do adaptador, estado explícito no nó |
| Fricção de instalação | Médio | Onboarding que detecta e instala o que falta, incluindo Git for Windows (que o Claude Code usa para a ferramenta Bash) |
| Injeção por conteúdo | Médio | Conteúdo lido é sempre dado, nunca instrução; ação externa passa por aprovação |
| Desempenho do canvas | Médio | Virtualizar nós fora da viewport, agrupar deltas em lotes de ~50 ms, limitar histórico renderizado |

---

## 12. Fora do v1, deliberadamente

- **Nuvem e multiusuário** — muda tudo. Se der certo local, vira v2 com o app como cliente.
- **Mobile** — exige backend. Substituto barato: notificação do Windows quando um agente levanta a mão.
- **Portais de dispositivo** — emulador Android e simulador iOS são coisa de dev-tool.
- **Copiloto local tipo Ombro** — bonito, não essencial.
- **Marketplace de papéis** — só faz sentido com usuários. Antes: pasta de papéis em Markdown, exportável.
- **Backend de cobrança e contas de usuário** — a chave é a do dono e o uso é interno. Existir
  servidor no produto muda a arquitetura, não só um marco; é decisão para o dia da divulgação.

---

## 13. Fontes técnicas

- Claude Code — [headless e stream-json](https://code.claude.com/docs/en/headless) ·
  [sessões](https://code.claude.com/docs/en/sessions) ·
  [permissões](https://code.claude.com/docs/en/permissions) ·
  [MCP](https://code.claude.com/docs/en/mcp) ·
  [Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview)
- Codex CLI — [modo não-interativo](https://learn.chatgpt.com/docs/non-interactive-mode)
- [MCP no Windows — On-Device Agent Registry](https://learn.microsoft.com/en-us/windows/ai/mcp/overview)
- [Model Context Protocol](https://modelcontextprotocol.io/)
- [Documentação do Maestri](https://www.themaestri.app/en/docs/intro) — referência de comportamento
- [Emdash](https://github.com/generalaction/emdash) — código de referência Apache-2.0 (PTY no Windows, detecção de CLIs)
