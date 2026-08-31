// Teste de fumaça da interface do M0.
//
// Roda o front construído (modo navegador, com o núcleo falso) num Chromium
// e exercita as interações que definem o marco: mover, ampliar, arrastar nó,
// ligar dois nós. Não substitui teste do Tauri — verifica o canvas.
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

console.log("\nM0 — teste de fumaça\n");

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
    ? "\n10 verificações, todas passaram.\n"
    : `\n${falhas.length} falha(s): ${falhas.join(", ")}\n`,
);
process.exit(falhas.length === 0 ? 0 : 1);
