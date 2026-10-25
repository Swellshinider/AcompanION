<!-- markdownlint-disable-file MD033 MD041 -->
<h1 align="center">AcompanION</h1>
<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT" /></a>
</p>

<p align="center">
  <a href="README.md">English</a> | Português (Brasil)
</p>

<p align="center">
  Um companheiro de Windows para AION 2 que lê o tráfego do jogo de forma passiva e acompanha cada personagem que você joga.
</p>

> **Status: em desenvolvimento inicial.** O decodificador de protocolo, o motor ao vivo e o app de desktop (Tauri + Angular) funcionam e são distribuídos como instalador para Windows (sem assinatura, então o SmartScreen avisa na primeira instalação). Os dados curados do jogo (nomes, limites, resets) são parciais, então alguns contadores e diárias ainda não aparecem. Veja [`docs/status.md`](docs/status.md) para saber exatamente em que pé as coisas estão (em inglês).

## Recursos

Funcionando hoje:

- **Captura passiva e decodificador próprio.** Captura via Npcap de toda conexão do AION 2, remontagem de TCP, decodificação de frames e LZ4, sem precisar de chaves.
- **Painel por personagem.** Cada personagem visto, com tickets, energia, diárias e semanais feitas em relação ao limite, GS (nível de item) e poder de combate; atualiza ao vivo e sobrevive a reinicializações.
- **Pets.** A coleção de pets da conta (almas, níveis, quais estão mais perto do próximo nível), um mapa de farm dos monstros que dropam almas de pet, um radar ao vivo dos monstros à vista e sua última posição conhecida, e uma rota de farm sugerida para os pets que você ainda não maximizou.
- **Overlay.** Uma janela móvel, redimensionável e translúcida com o radar, as diárias ou o medidor de DPS, cada um um overlay próprio que pode ficar aberto ao mesmo tempo. Opacidade, sempre no topo e deixar os cliques passarem são configurações de cada um.
- **Medidor de DPS.** Lutas ordenadas por dano com DPS, participação, críticos, cura e dano sofrido, um detalhe por jogador (habilidades, alvos) e um histórico salvo.
- **Interface traduzível.** Todo texto visível ao usuário é extraído para tradução. Inglês e português do Brasil já vêm no app, escolhidos em Configurações (ou pelo idioma do sistema); toda mudança de texto é traduzida para todos os idiomas na mesma mudança (veja [`docs/development.md`](docs/development.md), Translations).

## Como funciona

1. **Captura passiva.** Os pacotes são lidos pelo [Npcap](https://npcap.com/), carregado em tempo de execução (não precisa de SDK). Sem leitura de memória, sem injeção, sem enviar nenhuma entrada ao jogo.
2. **Decodificador próprio.** O tráfego do servidor para o cliente tem tamanho delimitado e é comprimido com LZ4, mas não é criptografado, então é decodificado sem chaves (veja [`docs/protocol.md`](docs/protocol.md)).
3. **Estado do personagem.** Os eventos decodificados atualizam um estado por personagem (identidade, poder de combate, contadores do checklist, pets) que é salvo em `%APPDATA%\AcompanION\characters.json`. As lutas são salvas em `meter.json`, ao lado dele.

## Instalação

Baixe `AcompanION_<versão>_x64-setup.exe` da [última release](https://github.com/Swellshinider/AcompanION/releases/latest) e execute. Ele instala para o usuário atual (sem pedir administrador) e você remove em Configurações > Aplicativos. O instalador procura o runtime do [Npcap](https://npcap.com/) e, se ele não estiver instalado, oferece baixar e executar o instalador oficial do Npcap (você clica pelo assistente dele) ou abrir a página de download. A captura precisa de direitos de administrador (o app oferece "Reiniciar como administrador").

Ao iniciar, o app verifica a última release; quando existe uma mais nova, um aviso oferece "Atualizar e reiniciar".

## Primeiros passos

### Requisitos

- Windows 11 com o runtime do [Npcap](https://npcap.com/) (as opções padrão do instalador servem);
- [Rust](https://www.rust-lang.org/) 1.97 ou mais novo (edition 2024);
- [Task](https://taskfile.dev/) para os atalhos do repositório (opcional, veja abaixo);
- [Node](https://nodejs.org/) 24 e [pnpm](https://pnpm.io/) 11, para o app de desktop (`pnpm install` uma vez);
- Um terminal **elevado** (Administrador) para a captura ao vivo;
- Opcional, para os scripts de análise: [uv](https://docs.astral.sh/uv/).

### Compilar e executar

```powershell
git clone https://github.com/Swellshinider/AcompanION.git
cd AcompanION
task build
task devices      # lista os adaptadores do Npcap
task live         # terminal elevado; Enter para parar
```

`task live` acompanha toda conexão do jogo, quer você o inicie antes ou depois do jogo, e um início no meio do fluxo também é decodificado. Cada personagem alterado é impresso como uma linha JSON e gravado no arquivo de estado. A saída contém nomes de personagens, então não a cole em lugar nenhum.

Variantes úteis:

```powershell
task live -- --seconds 60                          # para depois de um minuto
task live -- --replay captures/x.pcap --state out.json   # reproduz uma captura, sem adaptador
task live -- --device "<nome do adaptador>"        # escolhe o adaptador de `task devices`
```

### App de desktop

```powershell
pnpm install
task app          # terminal elevado para capturar; sem elevação mostra os personagens salvos e oferece "Reiniciar como administrador"
task app:build    # exe de release, sem instalador (o instalador é gerado pelo workflow de release)
```

O painel lista todos os personagens vistos com o que falta coletar e atualiza ao vivo. Ele lê e grava o mesmo arquivo de estado que o `task live`.

### Sem o Task

Toda tarefa é um comando simples; execute-o da raiz do repositório. Por exemplo, `task live` é:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin re -- live
```

e `task app` é `pnpm tauri dev`. `task -n <nome>` mostra o comando exato de qualquer tarefa.

## Desenvolvimento

As mesmas ferramentas acima bastam; as CLIs do Angular e do Tauri são dependências de desenvolvimento instaladas por `pnpm install`.

| O quê | Comando |
| --- | --- |
| Lint (clippy, avisos como erros) | `task lint` |
| Testes unitários do Rust | `task test` |
| Testes unitários do frontend | `task web:test` |
| Lint + Rust + testes do frontend (rode antes de cada commit) | `task check` |
| Testes que precisam do runtime do Npcap | `task test:ignored` |
| Compila o crate e a ferramenta `re` | `task build` |
| Extrai os textos traduzíveis | `task i18n` |

### Lançando uma versão

A versão fica só em `src-tauri/Cargo.toml`. Configuração única: `pnpm tauri signer generate -w <caminho>`, coloque a chave pública em `plugins.updater.pubkey` de `src-tauri/tauri.conf.json` e adicione a chave privada e sua senha como os secrets do repositório `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Nunca faça commit da chave privada: sem ela ninguém consegue distribuir atualizações, e quem a tiver pode.

1. Aumente `version` em `src-tauri/Cargo.toml` (e `Cargo.lock`) por um pull request e faça o merge.
2. Na `main`: `git tag vX.Y.Z && git push origin vX.Y.Z`. O workflow `Release` gera o instalador e cria uma release em **rascunho**.
3. Escreva as notas da release no rascunho (`gh release edit vX.Y.Z -R Swellshinider/AcompanION --notes-file <arquivo>`), partindo da seção desta versão em `docs/patch-notes.md` (depois ponha a data no título dela e abra a próxima, também em `docs/patch-notes.pt-BR.md`: o app mostra os dois em Configurações). As notas são em inglês e, depois de uma linha `---`, as mesmas notas em português sob `## Português (Brasil)`, partindo da seção da versão em `docs/patch-notes.pt-BR.md`. Elas são para jogadores, não para desenvolvedores: linguagem simples, o que é novo ou foi corrigido do ponto de vista do que o usuário vê, sem nomes de módulos, hashes de commit ou jargão. Mantenha os passos de instalação e o aviso do SmartScreen nos dois idiomas. Não cole a lista de commits.
4. Confira o rascunho e publique. Só então os apps instalados enxergam a atualização.

Correção para uma versão antiga: `git switch -c release/X.Y vX.Y.Z`, corrija, aumente a versão de patch, tag `vX.Y.(Z+1)` nessa branch.

### Fluxo de engenharia reversa

```powershell
# feche o jogo, inicie isto, DEPOIS abra o jogo e faça login
task record -- captures/my-login --seconds 600

task re -- dump   captures/my-login.pcap --summary    # streams remontados
task re -- decode captures/my-login.pcap              # eventos tipados como linhas JSON
task analyze -- chain captures/my-login.pcap          # análise avulsa em Python
```

`task record` grava `<prefixo>.pcap`. Ele contém dados da conta, por isso fica em `captures/`, que é ignorado pelo git. Nunca faça commit nem compartilhe capturas.

Veja [`docs/development.md`](docs/development.md) para o guia completo e [`docs/architecture.md`](docs/architecture.md) para como as peças se encaixam.

## Assistentes de código com IA

Se você é um LLM ou um assistente de código com IA, leia [`AGENTS.md`](AGENTS.md) e os documentos relevantes em [`docs/`](docs) antes de contribuir.

## Documentação

A documentação técnica está em inglês.

- [`docs/status.md`](docs/status.md) - em que pé as coisas estão, o que não foi verificado, o que vem a seguir
- [`docs/development.md`](docs/development.md) - configuração, tarefas, fluxo de captura, dados do jogo, traduções
- [`docs/architecture.md`](docs/architecture.md) - pipeline, módulos, threads, modelo de estado e o app de desktop
- [`docs/protocol.md`](docs/protocol.md) - tudo o que se sabe sobre o protocolo de rede, com evidências
- [`docs/decisions.md`](docs/decisions.md) - por que as coisas são como são
- [`docs/patch-notes.md`](docs/patch-notes.md) - o que mudou para os jogadores, a fonte das notas de release ([`patch-notes.pt-BR.md`](docs/patch-notes.pt-BR.md) é a cópia em português mostrada no app)
- [`tools/re/README.md`](tools/re/README.md) - o auxiliar de análise em Python

## Segurança e uso justo

A captura é passiva e somente leitura, a mesma abordagem de outros overlays da comunidade, mas ainda assim fica fora das ferramentas oficiais do jogo. Use nas suas próprias contas, por sua conta e risco. O AcompanION nunca envia nada ao jogo.

## Aviso legal

O AcompanION é um projeto independente e não oficial. Não é afiliado, endossado nem patrocinado pela NCSOFT. AION 2, seus nomes, ícones de classe e imagens de mapa são propriedade da NCSOFT e são usados aqui apenas para identificar o conteúdo do jogo. Use por sua conta e risco.

## Licença

[MIT](LICENSE).
