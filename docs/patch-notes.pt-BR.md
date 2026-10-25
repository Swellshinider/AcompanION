# Notas de atualização

Tradução em português do Brasil de [patch-notes.md](patch-notes.md), mostrada no app em Configurações › Ver o que há na sua versão. Cada linha nova vai nos dois arquivos, na mesma ordem, com as mesmas versões, seções e itens (o teste `patch-notes.spec.ts` confere).

## v0.2.2 (next)

### Novo

- Medidor: quatro nomes de monstros a mais (Draconute Handler, Executor of the Magical Sword, Abyss Rusty Rotan, Dimensional Core) e a dungeon Krao Cave agora são reconhecidos.
- Medidor: o poder de combate dos outros jogadores agora aparece ao lado do nome deles, no medidor e no overlay dele.
- Quando outra janela do AcompanION já está aberta, o aviso agora tem um botão **Fechar a outra e capturar aqui**, que fecha a outra e assume a captura.
- **Alerta do Festival Shugo**: um aviso no canto da tela que você escolher mostra quando o festival começa (toda hora cheia) enquanto o jogo está aberto. Em Configurações você liga ou desliga, escolhe o canto, define por quantos segundos ele fica (de 3 a 300), decide se um clique o fecha e silencia o som. Um botão mostra um alerta de teste.

## v0.2.1 (2026-10-25)

### Novo

- Um aviso informa quando o AcompanION não está sendo executado como administrador e oferece **Reiniciar como administrador**.
- Configurações: um botão **Ver o que há na sua versão** lista as mudanças de cada versão.
- Medidor: seu poder de combate agora aparece ao lado do seu nome, no medidor e no overlay dele. Os outros jogadores ainda não mostram o deles.

### Melhorado

- Overlay do medidor: o chefe agora tem um fundo escuro atrás dele, para ficar mais fácil de ler sobre o jogo.
- Overlay do medidor: a barra de título que aparece com o mouse em cima não cobre mais as informações do chefe.

## v0.2.0 (2026-10-25)

### Melhorado

- Configurações: reorganizadas. Radar, Diárias e Medidor agora são overlays separados, cada um com sua própria opacidade, sempre no topo, deixar os cliques passarem e ocultar automaticamente, e as três podem ficar abertas ao mesmo tempo.
- Overlay do medidor: visual novo, com barras arredondadas por jogador, uma barra de vida para o chefe e o dano por segundo completo. Ele não tem fundo, e a opacidade agora esmaece só as barras.
- Overlay: a borda e a barra de título só aparecem quando o mouse está sobre ele, e a borda branca fina da janela sumiu.
- Medidor: agora só as lutas contra chefes são contadas. Monstros comuns não iniciam mais uma luta, então os números de um chefe continuam na tela depois que ele morre, até o próximo chefe.
- Medidor: ele conhece mais nomes de chefes (por exemplo Neikel do Leste, Executora Kaira e Aparição de Zikel) em vez de mostrar "Monstro 1234".

### Corrigido

- Overlays: eles reabrem no tamanho e na posição em que você os deixou, em vez de encolherem um pouco a cada vez num segundo monitor com escala de tela diferente.
- Overlay de pets: "Próximo" agora indica um único monstro em vez de listar todos os monstros daquele ponto.
- Overlay de pets: uma alma que você coleta agora mostra seu pet e suas almas (ou Máx) mesmo quando o AcompanION foi aberto depois que você entrou no jogo.

## v0.1.3 (2026-10-23)

### Novo

- O botão Overlay agora é **Configurações**: uma página com as opções de overlay e as novas abaixo.
- Configurações: uma opção para manter o AcompanION rodando na bandeja do sistema quando você fecha a janela. Clique no ícone da bandeja para trazê-lo de volta, ou escolha Sair por lá.
- Configurações: um botão **Verificar atualizações agora** que diz se você está em dia.
- Configurações: a captura reabre sozinha quando a conexão de rede cai e volta. Você pode desativar isso.
- Você pode remover um personagem da lista: passe o mouse sobre o cartão e clique na lixeira vermelha (por exemplo depois de apagá-lo no jogo). Ele só volta se você entrar no jogo com ele de novo. Precisa da captura ativa.
- O AcompanION agora fala português do Brasil: o app inteiro, com os nomes de classes e contadores, aparece em português. Nomes de pets, monstros e habilidades continuam em inglês por enquanto.
- Configurações: uma opção de **Idioma**: Automático (segue o Windows, o padrão), English ou Português (Brasil). Uma nova escolha vale depois de reiniciar o AcompanION.
- O instalador procura o Npcap, que o AcompanION precisa para ler o tráfego do jogo. Se ele não estiver instalado, você pode deixar o instalador baixar e executar o instalador oficial do Npcap, ou abrir a página de download.

### Melhorado

- Medidor: durante uma luta, cada jogador mostra duas taxas, a atual (últimos 5 segundos) e a média; quando a luta termina, só a média fica. O total do grupo funciona do mesmo jeito, e o overlay também.
- Medidor: visual novo. Um DPS grande do grupo no topo, o chefe com sua barra de vida e uma etiqueta de Vitória ou Tentativa, e um ranking mais limpo com cabeçalho de colunas.
- Medidor: as lutas agora são uma lista à esquerda, agrupadas por dia, com o chefe, o resultado, a duração, o DPS do grupo e o seu próprio DPS e posição.
- Medidor: abrir um jogador ficou mais colorido: cada número tem sua cor, uma barra mostra como o dano se divide entre as habilidades, e cada linha de habilidade e de alvo tem uma barra. As habilidades mostram o ícone do jogo.
- Medidor: muito mais habilidades têm o nome real em vez de "Habilidade 123456".
- Medidor: você pode alternar o ranking entre dano causado, dano sofrido e cura.
- Minimapa do overlay: passar o mouse sobre um monstro mostra o nome dele ao lado do mouse.
- Minimapa do overlay: a linha da rota na parte de baixo agora mostra só o próximo monstro a farmar.
- Minimapa do overlay: com "Ocultar quando o jogo for fechado" ligado, o minimapa agora fica visível sempre que o jogo está aberto, como as diárias e o medidor, mesmo num mapa que o radar não conhece.
- O título AcompanION agora esmaece a parte "compan", para o nome ser lido como AION com um companheiro dentro.

## v0.1.2 (2026-10-22)

### Novo

- O medidor de combate conhece muito mais nomes de chefes e monstros (por exemplo Ferocious Horn Nuakum, Necromancer Duanka e Watchdog Kwapo) em vez de mostrar "Monstro 1234".

### Corrigido

- Medidor de combate: o dano dos espíritos de um Mestre Espiritual agora conta para o Mestre Espiritual, então o total, o DPS e a posição no ranking ficam certos.
- Medidor de combate: cada chefe agora tem sua própria luta, com seu próprio tempo e DPS. Dois chefes mortos em sequência apareciam como uma luta longa só.

## v0.1.1 (2026-10-22)

- Uma pequena atualização para conferir que as atualizações automáticas funcionam. Nada muda na aparência nem no comportamento do app.
