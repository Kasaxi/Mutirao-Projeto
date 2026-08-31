// Teste de fumaça da interface, M0 a M2.
//
// Roda o front construído (modo navegador, com o núcleo falso) num Chromium
// e exercita as interações que definem cada marco: mover, ampliar, arrastar e
// ligar nós (M0); pedir alguma coisa a um agente e ver a resposta chegando em
// bolhas, com o custo ao lado (M1); e aprovar uma gravação antes de ela
// acontecer (M2). Não substitui teste do Tauri — verifica a interface.
//
//   node testes-ui/fumaca.mjs

import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";

const RAIZ = new URL("../dist/", import.meta.url).pathname;
const PORTA = 4173;

const MIME = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
};

const servidor = createServer(async (req, res) => {
  const caminho = req.url === "/" ? "/index.html" : req.url.split("?")[0];
  // O Tauri não pede favicon; este servidor de teste evita o 404 que sujaria
  // a verificação de console limpo.
  if (caminho === "/favicon.ico") return res.writeHead(204).end();
  try {
    const dado = await readFile(join(RAIZ, caminho));
    res.writeHead(200, { "Content-Type": MIME[extname(caminho)] ?? "application/octet-stream" });
    res.end(dado);
  } catch {
    res.writeHead(404).end("não achei");
  }
});

const falhas = [];
const conferir = (nome, ok, detalhe = "") => {
  console.log(`${ok ? "  ok  " : " FALHA"}  ${nome}${detalhe ? ` — ${detalhe}` : ""}`);
  if (!ok) falhas.push(nome);
};

await new Promise((r) => servidor.listen(PORTA, r));

// Em CI o Chromium pode estar fora do lugar que o Playwright espera.
// CHROMIUM_BIN aponta para o binário; sem ela, usa o que veio com o pacote.
const navegador = await chromium.launch(
  process.env.CHROMIUM_BIN ? { executablePath: process.env.CHROMIUM_BIN } : {},
);
const pagina = await navegador.newPage({ viewport: { width: 1440, height: 900 } });

const erros = [];
pagina.on("pageerror", (e) => erros.push(String(e)));
pagina.on("console", (m) => m.type() === "error" && erros.push(m.text()));

await pagina.goto(`http://localhost:${PORTA}/`);
await pagina.waitForSelector(".no", { timeout: 5000 });

console.log("\nteste de fumaça\n\nM0 — canvas");

// 1. o canvas semeado aparece inteiro
const nos = await pagina.locator(".no").count();
const cabos = await pagina.locator(".cabo").count();
conferir("nós do canvas de demonstração", nos === 4, `${nos} nós`);
conferir("cabos entre os nós", cabos === 3, `${cabos} cabos`);

// 2. zoom ancorado no cursor
const zoomAntes = await pagina.locator(".zoom").innerText();
await pagina.mouse.move(700, 400);
await pagina.keyboard.down("Control");
await pagina.mouse.wheel(0, -300);
await pagina.keyboard.up("Control");
await pagina.waitForTimeout(120);
const zoomDepois = await pagina.locator(".zoom").innerText();
conferir("ctrl + rolar amplia", zoomAntes !== zoomDepois, `${zoomAntes} → ${zoomDepois}`);

// 3. rolar move a cena
const transformAntes = await pagina.locator(".mundo").getAttribute("style");
await pagina.mouse.wheel(0, 200);
await pagina.waitForTimeout(120);
const transformDepois = await pagina.locator(".mundo").getAttribute("style");
conferir("rolar move o canvas", transformAntes !== transformDepois);

// volta para 100% enquadrado, para as coordenadas ficarem previsíveis
await pagina.getByRole("button", { name: "Enquadrar" }).click();
await pagina.waitForTimeout(150);

// 4. arrastar um nó pelo cabeçalho
const alvo = pagina.locator(".no").first();
const antes = await alvo.boundingBox();
await pagina.mouse.move(antes.x + 80, antes.y + 16);
await pagina.mouse.down();
await pagina.mouse.move(antes.x + 260, antes.y + 116, { steps: 12 });
await pagina.mouse.up();
await pagina.waitForTimeout(150);
const depois = await alvo.boundingBox();
const andou = Math.round(depois.x - antes.x);
conferir("arrastar nó pelo cabeçalho", andou > 120, `andou ${andou}px`);

// 5. selecionar dá contorno
conferir("nó arrastado fica selecionado", (await pagina.locator(".no.selecionado").count()) === 1);

// 6. criar cabo arrastando da porta até outro nó
const cabosAntes = await pagina.locator(".cabo").count();
const origem = pagina.locator(".no").nth(3); // "Pasta do projeto", sem cabos
const destino = pagina.locator(".no").nth(1);
const cxOrigem = await origem.locator(".porta").boundingBox();
const bDestino = await destino.boundingBox();
await pagina.mouse.move(cxOrigem.x + 7, cxOrigem.y + 7);
await pagina.mouse.down();
await pagina.mouse.move(bDestino.x + bDestino.width / 2, bDestino.y + bDestino.height / 2, {
  steps: 14,
});
await pagina.mouse.up();
await pagina.waitForTimeout(200);
const cabosDepois = await pagina.locator(".cabo").count();
conferir("ligar dois nós arrastando da porta", cabosDepois === cabosAntes + 1,
  `${cabosAntes} → ${cabosDepois}`);

// 7. criar nó pela barra
const nosAntes = await pagina.locator(".no").count();
await pagina.getByRole("button", { name: "Nota" }).click();
await pagina.waitForTimeout(150);
conferir("botão da barra cria nó", (await pagina.locator(".no").count()) === nosAntes + 1);

// 8. renomear com dois cliques
const novo = pagina.locator(".no.selecionado");
await novo.locator(".no-cabecalho").dblclick();
await pagina.keyboard.press("Control+A");
await pagina.keyboard.type("Ata da reunião");
await pagina.keyboard.press("Enter");
await pagina.waitForTimeout(120);
conferir("renomear com dois cliques",
  (await novo.locator(".no-nome").innerText()) === "Ata da reunião");

// 9. Delete remove
const antesDel = await pagina.locator(".no").count();
await pagina.keyboard.press("Delete");
await pagina.waitForTimeout(150);
conferir("Delete remove o selecionado", (await pagina.locator(".no").count()) === antesDel - 1);

// 10. nada explodiu no console
conferir("console limpo", erros.length === 0, erros.slice(0, 2).join(" | "));

// ============================================================== M1 =========
// Critério de pronto do marco: "peço 'resuma este PDF' e vejo a resposta
// chegando em bolhas, com o custo ao lado". É isto que a seção abaixo mede.
//
// Recarrega para começar de um canvas limpo — os testes do M0 remexeram nele.

await pagina.reload();
await pagina.waitForSelector(".no");
await pagina.getByRole("button", { name: "Enquadrar" }).click();
await pagina.waitForTimeout(200);

console.log("\nM1 — conversa");

const agente = pagina.locator(".no-agente").first();
const campo = agente.locator(".conversa-campo");

conferir("nó de agente abre em face conversa", (await campo.count()) === 1);
// innerText devolve o texto RENDERIZADO, e `.selo` tem text-transform:
// uppercase — comparar sem baixar a caixa falha por um motivo que não é o que
// o teste quer medir.
const selo = (await pagina.locator(".selo.alerta").innerText()).toLowerCase();
conferir("adaptador falso se anuncia na barra", selo.includes("falso"), selo);

// 12. pergunta entra na hora, sem esperar o backend
await campo.fill("resuma este PDF");
await campo.press("Enter");
await pagina.waitForSelector(".no-agente .bolha.usuario", { timeout: 3000 });
conferir("a pergunta vira bolha no ato", (await agente.locator(".bolha.usuario").count()) >= 1);

// 13. o nó fica pensando, e pensando não pede socorro.
// A bolha do usuário é otimista: aparece antes de o núcleo confirmar. Por isso
// o estado se espera, não se afirma — afirmar aqui é corrida, e uma corrida
// que passa nove vezes em dez é pior que um teste vermelho.
await pagina.waitForSelector(".no-agente .sinal.pensando", { timeout: 3000 });
conferir("nó fica pensando", true);
conferir(
  "pensando não acende o ponto de atenção",
  (await agente.locator(".sinal.atencao").count()) === 0,
);

// 14. um turno por vez: o campo não aceita a segunda pergunta
conferir("campo bloqueia durante o turno", await campo.isDisabled());

// 15. ação do agente vira card, não linha de log
await pagina.waitForSelector(".no-agente .card-acao", { timeout: 5000 });
const card = await agente.locator(".card-acao").first().innerText();
conferir("ação vira card com o alvo legível", card.includes("contrato-v3.docx"), card.trim());

// ============================================================== M2 =========
// "o agente monta um arquivo na minha pasta e eu aprovo a gravação antes de
// acontecer". O turno PARA no card — não é um aviso, é um bloqueio.

console.log("\nM2 — aprovação");

await pagina.waitForSelector(".no-agente .card-aprovacao", { timeout: 8000 });
const licenca = agente.locator(".card-aprovacao");
conferir(
  "gravação para o turno num card",
  (await licenca.locator(".aprovacao-resumo").innerText()).includes("resumo-do-contrato.md"),
  await licenca.locator(".aprovacao-resumo").innerText(),
);
conferir(
  "o card diz o tamanho do que vai ser gravado",
  /linhas/.test(await licenca.locator(".aprovacao-detalhe").innerText()),
);
conferir("o nó pede atenção enquanto espera", (await agente.locator(".sinal.atencao").count()) === 1);
conferir("o campo continua bloqueado com card aberto", await campo.isDisabled());

// dá para ver o conteúdo antes de decidir
await licenca.getByRole("button", { name: "ver o que muda" }).click();
conferir(
  "dá para ler o conteúdo antes de aprovar",
  (await licenca.locator(".aprovacao-previa").innerText()).includes("Cláusula de reajuste"),
);

// a caixa "não perguntar de novo" aparece para gravação
conferir(
  "oferece não perguntar de novo para gravar",
  (await licenca.locator(".aprovacao-lembrar input").count()) === 1,
);

// Retrato do M2: o card aberto, com o turno parado atrás dele.
await pagina.screenshot({ path: "testes-ui/aprovacao.png" });

await licenca.getByRole("button", { name: "aprovar" }).click();
await pagina.waitForTimeout(150);
conferir("o card some depois do clique", (await agente.locator(".card-aprovacao").count()) === 0);

// 16. o turno termina sozinho e devolve o campo
await pagina.waitForFunction(
  () => {
    const c = document.querySelector(".no-agente .conversa-campo");
    return c instanceof HTMLTextAreaElement && !c.disabled;
  },
  { timeout: 10000 },
);
conferir("turno termina e libera o campo", true);

// 17. a resposta chegou inteira
const resposta = await agente.locator(".bolha.agente").last().innerText();
conferir("resposta chega em bolha", resposta.includes("reajuste"), resposta.slice(0, 48));

// 18. custo ao lado, e não zerado
const custo = await agente.locator(".conversa-custo").innerText();
conferir("custo do turno aparece", /US\$\s*0,0095/.test(custo), custo);
const custoBarra = await pagina.locator(".custo-total").innerText();
conferir("custo sobe para a barra do workspace", /US\$\s*0,0095/.test(custoBarra), custoBarra);

// Retrato do marco: um turno inteiro na tela, com ação, resposta e custo.
await pagina.screenshot({ path: "testes-ui/conversa.png" });

// 19. face terminal: a mesma sessão, crua
await agente.getByRole("button", { name: "terminal" }).click();
await pagina.waitForTimeout(120);
const cru = await agente.locator(".conversa-cru").innerText();
conferir("face terminal mostra o fluxo cru", cru.includes("turno_concluido"), cru.slice(0, 40));
await agente.getByRole("button", { name: "conversa" }).click();

// 20. parar interrompe sem deixar o nó pedindo socorro
await campo.fill("e o anexo?");
await campo.press("Enter");
await pagina.waitForSelector(".no-agente .conversa-botao.parar", { timeout: 3000 });
await agente.locator(".conversa-botao.parar").click();
await pagina.waitForTimeout(150);
conferir("parar devolve o nó para pronto", !(await campo.isDisabled()));
conferir(
  "cancelar não é erro",
  (await agente.locator(".sinal.atencao").count()) === 0,
);
const sistema = await agente.locator(".aviso-sistema").last().innerText();
conferir("a conversa registra quem parou", sistema.includes("interrompido"), sistema.trim());

// 21. negar bloqueia, e a ação fica marcada como falha na conversa
await campo.fill("tente gravar de novo");
await campo.press("Enter");
await pagina.waitForSelector(".no-agente .card-aprovacao", { timeout: 8000 });
await agente.locator(".card-aprovacao").getByRole("button", { name: "negar" }).click();
await pagina.waitForFunction(
  () => {
    const c = document.querySelector(".no-agente .conversa-campo");
    return c instanceof HTMLTextAreaElement && !c.disabled;
  },
  { timeout: 10000 },
);
conferir(
  "negar marca a ação como falha",
  (await agente.locator(".card-acao.falhou").count()) >= 1,
);

// 22. "não perguntar de novo" dispensa o card na vez seguinte
await campo.fill("grave mais uma vez");
await campo.press("Enter");
await pagina.waitForSelector(".no-agente .card-aprovacao", { timeout: 8000 });
await agente.locator(".card-aprovacao .aprovacao-lembrar input").check();
await agente.locator(".card-aprovacao").getByRole("button", { name: "aprovar" }).click();
await pagina.waitForFunction(
  () => {
    const c = document.querySelector(".no-agente .conversa-campo");
    return c instanceof HTMLTextAreaElement && !c.disabled;
  },
  { timeout: 10000 },
);

await campo.fill("e de novo");
await campo.press("Enter");
// Agora o turno tem de ir até o fim sem parar em card nenhum.
await pagina.waitForFunction(
  () => {
    const c = document.querySelector(".no-agente .conversa-campo");
    const card = document.querySelector(".no-agente .card-aprovacao");
    return c instanceof HTMLTextAreaElement && !c.disabled && !card;
  },
  { timeout: 12000 },
);
conferir("depois da regra, a gravação não pede card de novo", true);

// ---- substrato: nota em arquivo e árvore de verdade ----------------------

console.log("\nM2 — substrato");

const TEXTO_DA_NOTA = "# Ata\n\nPrazo confirmado: 18 meses.";
const nota = pagina.locator(".no-nota").first();
const campoNota = nota.locator(".nota-campo");
await campoNota.click();
await campoNota.fill(TEXTO_DA_NOTA);
await pagina.waitForTimeout(900); // o debounce de gravação
conferir(
  "a nota diz em qual arquivo ela mora",
  /\.md$/.test(await nota.locator(".nota-arquivo").innerText()),
  await nota.locator(".nota-arquivo").innerText(),
);

const arvore = pagina.locator(".no-arquivos").first();
await arvore.locator(".arvore-item").first().waitFor({ timeout: 3000 });

// A nota gravou de verdade? A árvore é quem sabe: ela lê a pasta, não o
// estado do React. (Recarregar a página não serviria de prova aqui — o modo
// navegador guarda tudo em memória e some junto com a página, de propósito.)
await arvore.getByText("reler").click();
await pagina.waitForTimeout(200);
const linhaDaNota = arvore.locator(".arvore-item", { hasText: "Briefing.md" });
conferir(
  "o que foi escrito na nota chegou ao arquivo",
  (await linhaDaNota.locator(".arvore-tamanho").innerText()) === `${TEXTO_DA_NOTA.length} B`,
  await linhaDaNota.locator(".arvore-tamanho").innerText(),
);

await arvore.locator(".arvore-item").first().waitFor({ timeout: 3000 });
const primeiros = await arvore.locator(".arvore-nome").allInnerTexts();
conferir("a árvore lista a pasta com pastas primeiro", primeiros[0] === "contratos", primeiros.join(", "));

await arvore.getByText("contratos").click();
await pagina.waitForTimeout(200);
const dentro = await arvore.locator(".arvore-nome").allInnerTexts();
conferir("dá para entrar numa subpasta", dentro.includes("minuta.docx"), dentro.join(", "));
await arvore.locator(".arvore-caminho .aba").first().click();
await pagina.waitForTimeout(200);
conferir(
  "e voltar",
  (await arvore.locator(".arvore-nome").allInnerTexts())[0] === "contratos",
);

// 23. e nada explodiu durante tudo isso
conferir("console limpo depois do turno", erros.length === 0, erros.slice(0, 2).join(" | "));

// Retrato do estado limpo, não do canvas remexido pelos testes acima.
await pagina.reload();
await pagina.waitForSelector(".no");
await pagina.getByRole("button", { name: "Enquadrar" }).click();
await pagina.waitForTimeout(250);
await pagina.screenshot({ path: "testes-ui/canvas.png" });

await navegador.close();
servidor.close();

console.log(
  falhas.length === 0
    ? "\nTodas as verificações passaram.\n"
    : `\n${falhas.length} falha(s): ${falhas.join(", ")}\n`,
);
process.exit(falhas.length === 0 ? 0 : 1);
