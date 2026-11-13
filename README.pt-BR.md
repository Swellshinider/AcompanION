<!-- markdownlint-disable-file MD033 MD041 -->
<h1 align="center">AcompanION</h1>
<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License: MIT" /></a>
</p>

<p align="center">
  <a href="README.md">English</a> | Português (Brasil)
</p>

Um companheiro de Windows para AION 2 que lê o tráfego do jogo de forma passiva e acompanha cada personagem que você joga.

> **Status: em desenvolvimento inicial.** O decodificador de protocolo, o motor ao vivo e o app de desktop (Tauri + Angular) funcionam e são distribuídos como instalador para Windows (sem assinatura, então o SmartScreen avisa na primeira instalação). Os dados curados do jogo (nomes, limites, resets) são parciais, então alguns contadores e diárias ainda não aparecem. Veja [`docs/agents/status.md`](docs/agents/status.md) para saber exatamente em que pé as coisas estão (em inglês).

## Primeiros passos

### Requisitos

- Windows 11 com o runtime do [Npcap](https://npcap.com/) (as opções padrão do instalador servem).
- Direitos de administrador para captura ao vivo; o app oferece "Reiniciar como administrador".

### Instalação

Baixe `AcompanION_<versão>_x64-setup.exe` da [última release](https://github.com/Swellshinider/AcompanION/releases/latest) e execute. Ele instala para o usuário atual (sem pedir administrador). O instalador procura o runtime do [Npcap](https://npcap.com/) e, se ele não estiver instalado, oferece baixar e executar o instalador oficial do Npcap (você clica pelo assistente dele) ou abrir a página de download. A captura precisa de direitos de administrador (o app oferece "Reiniciar como administrador").

Ao iniciar, o app verifica a última release; quando existe uma mais nova, um aviso oferece "Atualizar e reiniciar".

### Desinstalação

Remova o AcompanION em Configurações > Aplicativos do Windows.

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

## Desenvolvimento

Veja o [guia de desenvolvimento](docs/development.md) para requisitos, compilação a partir do código, tarefas, análise de capturas, traduções e [lançamento de versões](docs/development.md#releasing), e o [guia de contribuição](docs/contributing.md) para o fluxo de trabalho. Os guias técnicos estão em inglês.

## Assistente de código com IA

Leia [AGENTS.md](AGENTS.md), a [especificação do projeto](docs/agents/specification.md) e o [status atual](docs/agents/status.md) antes de contribuir.

## Documentação

Veja o [índice da documentação](docs/README.md) para guias técnicos e contexto para agentes, incluindo [arquitetura](docs/architecture.md), [evidências do protocolo](docs/protocol.md), [decisões](docs/decisions.md), [notas para os jogadores](docs/patch-notes.pt-BR.md) ([inglês](docs/patch-notes.md)) e o [auxiliar de análise](tools/re/README.md). A documentação técnica está em inglês.

## Políticas do projeto

- [Como contribuir](CONTRIBUTING.md)
- [Relato de vulnerabilidades](SECURITY.md)

### Segurança e uso justo

A captura é passiva e somente leitura, a mesma abordagem de outros overlays da comunidade, mas ainda assim fica fora das ferramentas oficiais do jogo. Use nas suas próprias contas, por sua conta e risco. O AcompanION nunca envia nada ao jogo.

### Aviso legal

O AcompanION é um projeto independente e não oficial. Não é afiliado, endossado nem patrocinado pela NCSOFT. AION 2, seus nomes, ícones de classe e imagens de mapa são propriedade da NCSOFT e são usados aqui apenas para identificar o conteúdo do jogo. Use por sua conta e risco.

## Licença

[MIT](LICENSE).
