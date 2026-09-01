# Confere se esta máquina tem o que o Mutirão precisa, e diz o que falta.
#
#   powershell -ExecutionPolicy Bypass -File verificar-windows.ps1
#
# Não instala nada e não muda nada: só olha e conta. É a versão de linha de
# comando do que o onboarding do M6 vai fazer na tela — e existe agora porque o
# primeiro contato com uma máquina limpa é onde o app tem mais chance de
# parecer quebrado sem estar.

$ErrorActionPreference = "Continue"
$faltando = @()

function Achei($nome, $comando, $porque, $como) {
    $c = Get-Command $comando -ErrorAction SilentlyContinue
    if ($c) {
        $versao = ""
        try { $versao = (& $comando --version 2>&1 | Select-Object -First 1) } catch { }
        Write-Host "  ok   $nome" -ForegroundColor Green -NoNewline
        Write-Host "  $versao"
        return $c
    }
    Write-Host "  FALTA $nome" -ForegroundColor Red -NoNewline
    Write-Host "  $porque"
    Write-Host "         instale: $como" -ForegroundColor DarkGray
    $script:faltando += $nome
    return $null
}

Write-Host ""
Write-Host "Mutirao - o que esta maquina tem" -ForegroundColor Cyan
Write-Host ""

# --- o que compila -----------------------------------------------------------

Achei "Node" "node" "front e ferramentas; precisa da 20 ou mais nova" `
    "https://nodejs.org (LTS)" | Out-Null

Achei "Rust" "cargo" "nucleo e casca; o SQLite e compilado do zero" `
    "https://rustup.rs" | Out-Null

# O linker do Windows nao e um comando no PATH: quem sabe onde ele esta e o
# vswhere, que a propria Microsoft instala junto com o Visual Studio Installer.
# Sem Build Tools o `cargo build` falha no fim, depois de compilar tudo - que e
# a pior hora possivel para descobrir.
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property displayName 2>$null
    if ($vs) {
        Write-Host "  ok   Build Tools" -ForegroundColor Green -NoNewline
        Write-Host "  $vs"
    } else {
        Write-Host "  FALTA Build Tools" -ForegroundColor Red -NoNewline
        Write-Host "  o Visual Studio esta ai, mas sem a carga C++"
        Write-Host "         instale: no Visual Studio Installer, marque 'Desenvolvimento para desktop com C++'" -ForegroundColor DarkGray
        $faltando += "Build Tools"
    }
} else {
    Write-Host "  FALTA Build Tools" -ForegroundColor Red -NoNewline
    Write-Host "  e o que linka o Rust no Windows"
    Write-Host "         instale: https://visualstudio.microsoft.com/visual-cpp-build-tools/" -ForegroundColor DarkGray
    Write-Host "         marque 'Desenvolvimento para desktop com C++'" -ForegroundColor DarkGray
    $faltando += "Build Tools"
}

# --- o que o app usa em tempo de execucao ------------------------------------

Achei "Git" "git" "os rascunhos precisam dele; sem ele o app sobe e avisa na barra" `
    "https://git-scm.com/download/win" | Out-Null

$claude = Achei "Claude Code" "claude" "sem ele o app roda no adaptador falso e avisa na barra" `
    "npm install -g @anthropic-ai/claude-code"

# Este e o ponto exato que o conserto do `processo.rs` existe para resolver:
# instalada pelo npm, a CLI e um .cmd, e o CreateProcess do Windows so
# acrescenta .exe ao procurar no PATH. Mostrar o caminho resolvido aqui e a
# diferenca entre "nao achei o Claude Code" e saber por que.
if ($claude) {
    Write-Host "         o app vai abrir: $($claude.Source)" -ForegroundColor DarkGray
    if ($claude.Source -like "*.cmd") {
        Write-Host "         (e um .cmd do npm - e justamente o caso que o app aprendeu a achar)" -ForegroundColor DarkGray
    }
}

# WebView2 ja vem no Windows 11. Vale conferir porque em Windows 10 nao vem, e
# a falta dele da uma janela branca sem mensagem nenhuma.
$chaves = @(
    "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
    "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
)
$webview = $null
foreach ($k in $chaves) {
    if (Test-Path $k) {
        $webview = (Get-ItemProperty $k -ErrorAction SilentlyContinue).pv
        if ($webview) { break }
    }
}
if ($webview) {
    Write-Host "  ok   WebView2" -ForegroundColor Green -NoNewline
    Write-Host "  $webview"
} else {
    Write-Host "  ?    WebView2" -ForegroundColor Yellow -NoNewline
    Write-Host "  nao achei no registro - no Windows 11 costuma vir de fabrica"
    Write-Host "         se a janela do app abrir em branco, instale:" -ForegroundColor DarkGray
    Write-Host "         https://developer.microsoft.com/microsoft-edge/webview2/" -ForegroundColor DarkGray
}

# --- veredito ----------------------------------------------------------------

Write-Host ""
if ($faltando.Count -eq 0) {
    Write-Host "Tudo no lugar. O caminho mais barato, nesta ordem:" -ForegroundColor Green
    Write-Host ""
    Write-Host "  npm install"
    Write-Host "  npm run dev                                    # so o front, no navegador, sem gastar nada"
    Write-Host '  $env:MUTIRAO_ADAPTADOR="falso"; npm run app     # o app de verdade, com roteiro no lugar do modelo'
    Write-Host "  Remove-Item Env:MUTIRAO_ADAPTADOR; npm run app  # o real, com a CLI e a sua chave"
} else {
    Write-Host "Falta instalar: $($faltando -join ', ')" -ForegroundColor Red
    Write-Host "Depois de instalar, ABRA UM TERMINAL NOVO - o PATH so vale para" -ForegroundColor Yellow
    Write-Host "terminal aberto depois da instalacao, e este e o motivo numero um" -ForegroundColor Yellow
    Write-Host "de 'instalei e ele diz que nao achei'." -ForegroundColor Yellow
}
Write-Host ""
