# Interface BIBFRAME: listagem, catalogação e exemplares (especificação conceitual)

> Status: v1. A v0 (2026-11-13) partiu do esboço interativo `bibframe-E.html` ("Esboço Teórico v4"); a v1 incorpora o esboço `elzevir-esboco-v9.html` (coluna de Autoridades, estações de captura no bloco, muro Acervo ↔ Leitores, `Esc` direto para a visão geral), com os ajustes aprovados por Paulo em 2026-11-13 (§16 a §19). **Não há código de produção aqui.** É o contrato de interação para a Onda 1 e a Onda 2 do frontend ([visão de arquitetura](ARCHITECTURE_VISION.md), A.3 e B.6) e para as rotas e comandos que as servem.
>
> Pré-condição de implementação (visão, A.4 item 4): núcleo BIBFRAME e testes de ida e volta MARC21 verdes (tarefas 1.5 e 1.6).

## Sumário

1. [Princípios](#1-princípios)
2. [Anatomia da linha-família](#2-anatomia-da-linha-família)
3. [Estados, foco 80/20 e trilha](#3-estados-foco-8020-e-trilha)
4. [Edição no lugar e perfis por categoria](#4-edição-no-lugar-e-perfis-por-categoria)
5. [Grade de exemplares](#5-grade-de-exemplares)
6. [Criação rápida e catalogação derivada](#6-criação-rápida-e-catalogação-derivada)
7. [Mesclagem e órfãos](#7-mesclagem-e-órfãos)
8. [Busca e listagem são a mesma tela](#8-busca-e-listagem-são-a-mesma-tela)
9. [Ação na tela → comando → tabelas → MARC](#9-ação-na-tela--comando--tabelas--marc)
10. [Lacunas do modelo](#10-lacunas-do-modelo)
11. [Renderização e desempenho](#11-renderização-e-desempenho)
12. [Acessibilidade, idioma e privacidade](#12-acessibilidade-idioma-e-privacidade)
13. [Atalhos de teclado](#13-atalhos-de-teclado)
14. [Fases e critérios de aceite](#14-fases-e-critérios-de-aceite)
15. [O que muda em relação ao esboço](#15-o-que-muda-em-relação-ao-esboço)
16. [Coluna de Autoridades e ligação por papel](#16-coluna-de-autoridades-e-ligação-por-papel)
17. [Estações de captura no bloco: Z39.50/SRU e IA](#17-estações-de-captura-no-bloco-z3950sru-e-ia)
18. [Muro Acervo ↔ Leitores e empréstimo](#18-muro-acervo--leitores-e-empréstimo)
19. [Decisões do esboço v9](#19-decisões-do-esboço-v9)
20. [Mecânicas pelo olhar do catalogador](#20-mecânicas-pelo-olhar-do-catalogador)

## 1. Princípios

1. **Uma tela só** para acervo, resultado de busca, catalogação e exemplares. Nada de "tela de busca", "tela do registro" e "tela de exemplares" separadas: o que muda é o **foco** dentro da mesma lista.
2. **O grafo é o registro.** O usuário vê e edita entidades BIBFRAME (Agente, Hub, Obra, Instância, Item) ligadas por cordinhas, nunca etiquetas MARC. O MARC aparece só numa aba de inspeção, só leitura, e na exportação.
3. **Foco sem perder o contexto** (80/20): ampliar um bloco nunca esconde de onde ele vem.
4. **Teclado primeiro, mouse bem-vindo.** Toda ação com mouse tem atalho; toda ação de arrastar tem alternativa por teclado (§12).
5. **Destrutivo só com confirmação, e reversível.** Mesclar e excluir passam por uma decisão explícita; o padrão é mandar para a quarentena, não apagar (§7).
6. **Servidor renderiza, o navegador anima.** HTML vem do Axum (Askama + HTMX); o JavaScript cuida só de cordinhas, foco, grade e atalhos (§11).
7. **Uma regra, um lugar** (`CLAUDE.md`): a interface não decide regra de negócio. Validação de código de barras, de mesclagem e de perfil vive nos comandos da escritora; a tela só mostra a recusa.

## 2. Anatomia da linha-família

Cada linha da listagem é uma **família**: o menor conjunto ligado que faz sentido mostrar junto.

- **Unidade da família**: o Hub, quando a Obra pertence a um; senão, a Obra. Uma família com Hub mostra todas as Obras do Hub (as traduções lado a lado, como "Obra • Português" e "Obra • Inglês" no esboço).
- **Colunas**, da esquerda para a direita, numa grade CSS de 5 trilhas:

| Coluna | Conteúdo no modo geral | Origem no banco |
|---|---|---|
| Autoridades | Agentes (nome autorizado, datas, papel) e assuntos ligados à família; botão "+ Autoridade" com busca (§16) | `agents` via `contributions` (Obra) e contribuições da Instância (§10); `subjects` via `work_subjects` |
| Matriz (Hub) | Título uniforme e criador | `hubs` |
| Obra | Título, agentes, CDD, idioma, etiquetas, até 3 assuntos e "+N" | `works`, `work_languages`, `work_subjects` |
| Instância | Capa (§6.4), título, imprenta, ISBN, edição, ícone da categoria | `instances`, `instance_identifiers` |
| Exemplares | Até 4 pílulas por Instância e "+N exemplares" | `items` |

- **Hub ausente**: a trilha do Hub tem largura `0` e as cordinhas ligam Entidades direto à Obra. Sem espaço vazio.
- **Pílula de exemplar**: `unidade | localização | nº de chamada | tombo`, com o **semáforo de balcão**:

| Cor | Situação | Regra (no domínio, nunca na tela) |
|---|---|---|
| Verde | Disponível | sem empréstimo aberto e todas as situações = 0 |
| Amarelo | Emprestado | empréstimo aberto (Fase 3; até lá não aparece) |
| Azul | Em trânsito ou reservado na estante | Fase 3 |
| Vermelho | Fora de circulação | `withdrawn`, `lost` ou `damaged` ≠ 0 |
| Cinza | Não emprestável | `not_for_loan` ≠ 0 |

  Cor nunca é a única pista: cada estado tem ícone e texto para leitor de tela (§12). O azul e o cinza são acréscimos ao esboço.
- **Ordem das pílulas**: disponíveis primeiro, depois por tombo. O "+N" abre a grade (§5).
- **Cordinhas**: uma curva Bézier por ligação, com rótulo curto da relação ("escrito por", "tradução de", "edição", "exemplares"), vindo do Fluent.

## 3. Estados, foco 80/20 e trilha

A tela é uma máquina de estados pequena, guardada na URL (para voltar, recarregar e compartilhar um link com o colega):

| Estado | URL | Larguras das 5 trilhas | Entrada | Saída |
|---|---|---|---|---|
| Geral | `/equipe/acervo?q=…` | ~16/16/22/23/23 % (Hub 0 se não há) | — | — |
| Foco no Hub | `…#hub-12` | 10/74/16/0/0 | clique ou `Enter` no Hub | `Esc` |
| Foco na Obra | `…#work-40` | 0/11/75/14/0 | clique ou `Enter` na Obra | `Esc` |
| Foco na Instância | `…#instance-77` | 0/0/11/76/13 | clique ou `Enter` na Instância | `Esc` |
| Grade de exemplares | `…#items-77/3` | 0/0/0/21/79 | clique numa pílula (linha 3) ou no "+N" | `Esc` |
| Decisão de mesclagem | (não muda a URL) | mantém | soltar uma cordinha (§7) | escolher ou `Esc` |

- As porcentagens são as do esboço e são **ajustáveis por tema**; o que é contrato é a proporção ~80 % foco / ~20 % contexto, com os vizinhos imediatos visíveis e ligados.
- **Só uma família em foco por vez.** As outras linhas continuam na lista, com opacidade reduzida, e não respondem a teclas de edição.
- **Escurecimento seletivo** (*dimming*): no foco de Instância e na grade, as Instâncias irmãs e seus exemplares ficam com opacidade 0,35 e `inert`, para ninguém tombar no exemplar errado.
- **Expansão vertical**: o bloco focado cresce para baixo e vira formulário (§4). A linha da família cresce junto; as linhas de baixo descem, sem sobrepor.
- **Trilha superior**: `Autoridades › Matriz › Obra › Instância › Exemplares`, com o nível atual em destaque e níveis ausentes (sem Hub) omitidos. Cada item é um link para o estado correspondente: é a trilha que faz os saltos intermediários.
- **`Esc` em camadas** (v1, esboço v9): `Esc` fecha primeiro o que é passageiro, nesta ordem: a célula em edição da grade, a minicaixinha de empréstimo (§18), a estação de captura (§17), o diálogo de mesclagem. Sem nada passageiro aberto, `Esc` volta **direto à visão geral**, sem subir nível por nível. No lado do leitor (§18), `Esc` atravessa o muro de volta ao acervo. "Concluir" e clicar no fundo da linha fazem o mesmo que esse último `Esc`.
- **Edição pendente**: sair de um bloco com alterações não salvas (por `Esc`, "Concluir", clique no fundo ou na trilha) salva automaticamente se o formulário é válido; se não é, a saída é bloqueada com o campo inválido destacado (sem diálogo do navegador). Edição pendente nunca é descartada em silêncio.

## 4. Edição no lugar e perfis por categoria

### 4.1 Mecânica

1. No foco, os valores do bloco viram campos editáveis **no mesmo lugar**, na mesma ordem.
2. Abaixo deles, uma seção recolhível "Campos ainda não preenchidos" mostra o resto do **perfil da categoria** (`material_type`), na ordem do perfil.
3. No modo leitura, só aparecem campos preenchidos (visão, A.3 Onda 1 item 2).
4. **Concluir** (`Ctrl+Enter`) envia **um** comando com tudo o que mudou no bloco e o `row_version` lido. Recusa (validação ou edição concorrente) aparece junto do campo, e o formulário continua aberto.
5. Campos repetíveis (agentes, assuntos, identificadores, idiomas, etiquetas) são listas de *chips* com reordenação por teclado (`Alt+↑/↓`) e autocompletar nos vocabulários existentes (agentes, assuntos, `terms`).
6. Agente e assunto são **entidades**: digitar um nome sugere as existentes; escolher uma liga a ela; criar uma nova é explícito ("Criar agente 'Baldi, Paulo'"). Isso evita duplicatas na origem, antes de precisar de mesclagem.

### 4.2 Perfis

Os perfis são **dados tipados**, um arquivo por categoria, carregados na inicialização e conferidos em teste contra o modelo: cada campo do perfil aponta para uma propriedade do modelo, nunca para uma etiqueta MARC. A coluna "MARC" abaixo é só para catalogadores que pensam em MARC e para a ajuda contextual.

**Comuns a todas as categorias** (Obra): título, título uniforme/Hub, agentes com papel, idiomas, classificação (CDD), assuntos, resumo, público-alvo, etiquetas.

**Livro** (`book`), 18 campos:

| Bloco | Campo | Modelo | MARC (referência) |
|---|---|---|---|
| Obra | Título da obra | `works.title` | 240/245 |
| Obra | Agentes e papéis | `contributions` | 100/700 |
| Obra | Classificação (CDD) | `works.classification` + esquema `ddc` | 082 |
| Obra | Idiomas | `work_languages` | 041/008 |
| Obra | Assuntos | `work_subjects` | 6XX |
| Obra | Resumo | `works.summary` | 520 |
| Obra | Público-alvo | `works.extra.audience` (§10) | 521 |
| Obra | Etiquetas | §10 | — |
| Instância | Título na publicação | `instances.title` | 245 |
| Instância | Contribuidores da edição (ilustrador, tradutor, prefaciador) | §10 | 700 com `$4` |
| Instância | Edição | `instances.edition` | 250 |
| Instância | Local, editora, data | `instances.place/publisher/date` | 260/264 |
| Instância | Descrição física | `instances.extent` | 300 |
| Instância | Série | `instances.series` | 490 |
| Instância | ISBN | `instance_identifiers` | 020 |
| Instância | Notas | `instances.extra.notes` (§10) | 500 |
| Instância | Categoria | `instances.material_type` | líder/007 |
| Instância | Capa | §6.4 | — |

**Periódico** (`periodical`): troca ISBN por ISSN, acrescenta periodicidade (310), numeração (362) e "título anterior/posterior" (relações entre Obras `continuation_of`).
**Mapa** (`map`): escala (255), projeção, coordenadas; sem edição/série obrigatórias.
**Partitura** (`score`): meio de execução (382), forma musical, ISMN.
**Gravação sonora** e **vídeo**: duração (306), formato do suporte (338/007), intérpretes como contribuidores da edição.
**Digital** (`digital`): URL e condições de acesso (856), formato do arquivo, DOI.
**Material visual** e **misto**: título, agentes, descrição física, notas e assuntos; o resto sob demanda.

Cada perfil tem entre 12 e 20 campos (visão, A.1). Campos que o modelo ainda não tem estão em §10, com a migração que os cria; **nenhum campo entra num perfil antes de existir no modelo e no mapeamento MARC** (tarefa 1.6), para não quebrar a ida e volta.

## 5. Grade de exemplares

A coluna de exemplares, em foco, vira uma grade de planilha ancorada à Instância dona (~20 % à esquerda, com a cordinha).

- **Colunas**: situação (semáforo), tombo, código de barras, nº de chamada, unidade de origem, unidade atual, localização, coleção, tipo de item, nº do exemplar, preço, observações. Colunas visíveis e largura ficam salvas por usuário.
- **Teclado**: setas movem a célula; `Tab`/`Shift+Tab` andam na linha; `Enter` confirma e desce; `F2` ou digitar edita; `Esc` cancela a célula (e um segundo `Esc` sai da grade); `Ctrl+D` copia a célula de cima (tombamento em série); `Ctrl+Enter` salva o lote.
- **Leitor de código de barras**: funciona como teclado. Na coluna "código de barras", uma sequência rápida terminada em `Enter` vira a célula e desce para a próxima linha; na última linha, cria uma linha nova herdando unidade, localização, tipo e nº de chamada da anterior ("+ Novo exemplar (bipe)" do esboço).
- **Tombamento em lote**: "+N linhas" com tombo inicial e incremento (`2026/0001…0020`); os códigos de barras vêm do bipe ou de uma sequência.
- **Salvar**: as linhas alteradas vão num comando de lote só (`catalog.save_items`), atômico: ou todas entram ou nenhuma; cada recusa (código de barras repetido, unidade desconhecida) volta marcada na célula. Nada se salva célula a célula, para não fazer uma escrita por tecla.
- **Filtrar e ordenar** na própria grade (por situação, unidade, localização), sem sair do foco.
- **Circulação rápida** (Fase 3): na linha do exemplar, ações de balcão (emprestar a…, devolver, renovar) e o histórico **só para quem tem permissão de circulação**; o leitor nunca aparece na grade por padrão (§12).

## 6. Criação rápida e catalogação derivada

### 6.1 "+ Nova" por coluna

| Onde | Cria | Herda | Ligação |
|---|---|---|---|
| Coluna Obra | Obra nova no mesmo Hub (ex.: outra tradução) | Hub, agentes principais, CDD, assuntos | `hub_id` e, se for tradução, `translation_of` com a Obra de origem |
| Coluna Instância | Instância nova da Obra focada | título, categoria, idioma | `instances.work_id` |
| Grade | Linha de exemplar | unidade, localização, tipo, nº de chamada | `items.instance_id` |
| Sem foco (`Ctrl+N`) | Família nova vazia | — | — |

O bloco novo nasce já em edição, com o cursor no primeiro campo vazio do perfil.

### 6.2 Catalogação contínua (`Ctrl+N` em série)

Depois de **Concluir**, `Ctrl+N` abre outra família nova com os mesmos valores fixos da sessão (categoria, unidade, localização, tipo de item), para catalogar uma pilha de livros sem tirar a mão do teclado.

### 6.3 Catalogação derivada (ISBN, Z39.50, SRU)

É o ponto de extensão "fontes de rascunho" do doc 04. Na v1, a busca externa e a IA acontecem **dentro do bloco** focado, como estações de captura (§17); os passos abaixo continuam valendo para o primeiro campo da família nova.

1. Na família nova, o primeiro campo aceita **ISBN bipado** ou texto.
2. O servidor consulta as fontes configuradas (a própria base primeiro, para detectar duplicata; depois SRU/Z39.50 na Fase 4).
3. Os resultados aparecem como **blocos-rascunho** tracejados ao lado, com a origem ("Biblioteca Nacional", "LC").
4. Escolher um rascunho preenche os blocos Obra e Instância passando o MARC recebido por `marc_to_bibframe` (o mesmo caminho da importação). O MARC recebido vira o registro de origem da Instância.
5. **Duplicata**: se o ISBN já existe na base, a primeira sugestão é "Acrescentar exemplar à Instância existente", que leva direto à grade dela.

### 6.4 Capa

Opcional por configuração. Buscada pelo ISBN numa fonte configurável, **pelo servidor**, guardada em cache local por Instância e servida pelo próprio Elzevir. O navegador do balcão não chama serviços externos (privacidade e funcionamento sem internet).

## 7. Mesclagem e órfãos

### 7.1 Gesto

- Cada bloco tem uma **alça** na borda direita. Arrastar a alça e soltar sobre um bloco **do mesmo nível** abre a decisão. Soltar em nível incompatível não faz nada (a cordinha volta, com animação).
- **Alternativa por teclado** (acessibilidade): `M` no bloco focado entra em "modo mesclar"; setas escolhem o destino; `Enter` abre a decisão.

### 7.2 Decisão (o único diálogo da tela)

Mostra origem → destino, **o que será movido** (contagens: "3 Instâncias, 12 exemplares, 2 relações") e três opções:

1. **Transferir filhos e excluir a origem** (mescla limpa).
2. **Transferir filhos e mandar a origem para a quarentena** (padrão, destacada): nada é apagado.
3. **Cancelar** (`Esc`).

### 7.3 O que move, por nível

| Origem → destino | Filhos transferidos | Também |
|---|---|---|
| Agente → Agente | contribuições (Obra e Instância) e assuntos de agente | cabeçalho de origem vira forma variante do destino (§10) |
| Hub → Hub | Obras | — |
| Obra → Obra | Instâncias, relações entre Obras, idiomas e assuntos que o destino não tem | relações que apontavam para a origem passam a apontar para o destino |
| Instância → Instância | exemplares e identificadores que o destino não tem | registro de origem: o do destino fica; o da origem vai junto para a quarentena (ou some, na mescla limpa) |

### 7.4 Quarentena

- É um **estado**, não uma tabela à parte: o bloco ganha `quarantined_at` (§10) e some da listagem normal; aparece no filtro "Órfãos e quarentena".
- Da quarentena: **restaurar** (volta à listagem, sem filhos), **excluir de vez** (só sem filhos) ou **mesclar** de novo.
- Uma tarefa agendada lista quarentenas com mais de N dias para revisão; nunca apaga sozinha.
- **Desfazer**: toda mesclagem fica no `audit_log` com o antes e o depois; "Desfazer" fica disponível na barra de avisos até a próxima ação, e depois pela tela de auditoria.

## 8. Busca e listagem são a mesma tela

- A caixa de busca filtra e reordena **as famílias da mesma lista**; não existe outra tela de resultados.
- **Entradas**: texto livre (Tantivy), CDD (`823`, `82*`), ISBN, **bipe de código de barras** (vai direto ao foco na grade daquele exemplar), clique num chip de agente, assunto ou etiqueta (vira filtro), tombo.
- **Retorno do índice**: o Tantivy devolve ids de Instância com pontuação; o servidor agrupa por família (Hub ou Obra), ordena as famílias pela melhor Instância e renderiza as linhas. Na família, a Instância que casou vem primeiro e com destaque; as outras ficam recolhidas ("+2 edições").
- **Listagem sem busca**: ordem configurável (título, CDD/número de chamada, mais recentes); paginada por cursor.
- **Primeira linha útil para o balcão** (visão, A.2): título, autor principal, nº de chamada, categoria, localização e contagem disponível/emprestado vêm do índice e de uma consulta por página ao SQLite; a família inteira só carrega quando a linha entra na tela.
- Tudo do foco (§3), da edição (§4), da grade (§5) e da mesclagem (§7) funciona igual numa lista filtrada.

## 9. Ação na tela → comando → tabelas → MARC

Toda escrita é um comando tipado da escritora (`WriteCommand::Catalog`), registrado no `audit_log`. Nenhuma tela escreve SQL; o MARC nunca é guardado editado: a exportação o gera do modelo mais o registro original (`bibframe_to_marc`, tarefa 1.6, spec `docs/specs/catalogo/mapeamento-marc21.md` §1.5). O papel que a visão chamava de `raw_marc_extra` é o de `source_records` (B.2, aprovado).

| Ação | Comando | Tabelas | Efeito no MARC exportado |
|---|---|---|---|
| Editar campo da Obra | `catalog.update_work { id, row_version, mudanças }` | `works`, `work_languages`, `contributions`, `work_subjects`, `terms`, `agents`, `subjects` | só os campos daquela parte (130/240, 082, 041, 1XX/7XX, 6XX, 520) de **todas** as Instâncias da Obra |
| Editar campo da Instância | `catalog.update_instance { id, row_version, mudanças }` | `instances`, `instance_identifiers` | 245, 250, 26X, 300, 490, 020/022/024/035 daquela Instância |
| Nova Obra no Hub | `catalog.add_record` com `hub` e `relations` | `works`, `work_relations` | registro novo, sem registro de origem; Hub e relações ainda não vão para o MARC (spec de mapeamento, §6) |
| Nova Instância de uma Obra | `catalog.add_instance { work, … }` (novo) | `instances`, `instance_identifiers` | registro novo, gerado do zero |
| Editar células da grade | `catalog.save_items { instance, linhas }` (novo, em lote) | `items` | `952` daquela Instância: linha alterada mantém os subcampos do Koha (`$9`, `$6`), linha nova vira `952` novo |
| Mesclar Obras | `catalog.merge { level: Work, from, into, origin: Delete \| Quarantine }` (novo) | `instances.work_id`, `work_relations`, `work_languages`, `work_subjects`; `works.quarantined_at` ou `DELETE` | as Instâncias movidas passam a exportar os campos de Obra do destino |
| Mesclar Instâncias | idem, `level: Instance` | `items.instance_id`, `instance_identifiers`; `source_records` da origem acompanha a origem | o `952` dos exemplares movidos sai no registro do destino |
| Deixar órfão | parte do `merge` (`origin: Quarantine`) | `quarantined_at` | o órfão não é exportado nem indexado enquanto estiver em quarentena |
| Restaurar da quarentena | `catalog.restore { level, id }` (novo) | `quarantined_at = NULL` | volta a ser exportado |

Regras de integridade, todas no comando (não na tela):

- Recusa com `row_version` diferente do lido (`CatalogRefusal::Stale`), mostrada como "alterado por outra pessoa; recarregar".
- Mesclar exige o mesmo nível e instituição; Obra não se mescla com ela mesma nem com uma Obra que a cita em `work_relations` sem tratar o ciclo.
- Exemplares movidos mantêm id, código de barras e histórico de circulação (circulação liga só ao Item, visão A.2).
- Toda escrita gera evento na `effect_outbox` para o indexador (tarefa 1.8) reindexar as famílias afetadas.

## 10. Lacunas do modelo

O esboço mostra coisas que o esquema de hoje (migração 4) ainda não guarda. Cada uma vira migração, comando e mapeamento MARC **antes** de entrar na tela:

| Lacuna | Proposta | MARC |
|---|---|---|
| Contribuidores da edição (ilustrador, tradutor) | `instance_contributions` (como `contributions`, ligada à Instância) | 700/710 com `$4`, separados dos da Obra por regra de papel na 1.6 |
| Etiquetas ("adulto", "ficção") | vocabulário `Tag` em `terms` + `work_tags` | 690 local ou só no Elzevir (decidir com Paulo) |
| Público-alvo | **feito** (tarefa 2.1): `works.audience`, no OPAC | 521 |
| Notas | **feito** (tarefa 2.1): `instance_notes` (geral, conteúdo, local), no índice e no OPAC | 500, 505, 590 |
| Quarentena | `quarantined_at TEXT` em `hubs`, `works`, `instances`, `agents` | não exportado |
| Formas variantes de agente (após mesclagem) | `agent_variants (agent_id, heading)` | 400 nas autoridades (fase de autoridades) |
| Capa | arquivo em cache por Instância, fora do SQLite | — |
| Emprestado (amarelo) | `loans` (Fase 3) | 952 `$q` no Koha, calculado |
| Campos específicos de perfis (escala, ISMN, duração…) | colunas ou `extra` tipado, um por vez, com teste de ida e volta | 255, 024, 306… |
| Identificador de autoridade externa (VIAF, LCNAF, BN) | `agent_identifiers (agent_id, source, value)` e o mesmo para `subjects`, únicos por fonte | `$0`/`$1` nos 1XX/6XX/7XX; 024 nas autoridades |
| Local de publicação como autoridade | ligação opcional `instances.place_id → subjects` (Lugar), mantendo o texto da imprenta | 264 `$a` com `$0` |
| Origem de um rascunho aprovado | `source_records.origin` (`marc_file`, `z3950:<servidor>`, `sru:<servidor>`, `ai:<modelo>`) e, para IA, a marca por campo no `audit_log` | — |
| Ficha do leitor | `patrons` (Fase 3), com CPF guardado completo e mostrado mascarado; sem campo de "perfil de leitura"; histórico só com consentimento | — |

## 11. Renderização e desempenho

Meta: 60 FPS nas transições e resposta perceptível < 100 ms num computador simples de balcão; sem framework de SPA.

**Servidor** (Axum + Askama + HTMX):

- Cada linha-família é um fragmento HTML com chave `família:row_version` (cache de fragmentos por versão, doc 05 §3).
- Lista paginada por **cursor** (não `OFFSET`), 30 famílias por página; a próxima página entra com HTMX quando a última linha aparece (`hx-trigger="revealed"`), e páginas que saem muito da tela são descarregadas.
- Foco e edição trocam só o fragmento do bloco (`hx-swap` no bloco), nunca a página.

**Navegador** (CSS moderno e JS pequeno, em módulos sem dependência):

- Linhas com `content-visibility: auto` e `contain-intrinsic-size`, e `contain: layout paint`: o navegador não desenha nem calcula as linhas fora da tela.
- **Foco 80/20**: a linha é `display: grid` com `grid-template-columns` vindo de uma variável por estado (`data-focus="work"`). A transição é de `grid-template-columns` (como no esboço), **só na linha focada**. Com `prefers-reduced-motion`, a troca é imediata.
- **Troca de foco entre famílias** e entrada na grade: `document.startViewTransition` onde houver suporte, com `view-transition-name` só nos blocos envolvidos; sem suporte, a mesma mudança sem animação.
- **Cordinhas**: um `<svg>` por linha, em camada absoluta com `pointer-events: none` (menos nas alças). Os caminhos são recalculados a partir de `getBoundingClientRect` dos blocos:
  - por `ResizeObserver` na linha (cobre expansão vertical, mudança de janela e de fonte);
  - durante a transição de foco, a cada `requestAnimationFrame` **só da linha focada**, por no máximo a duração da transição (o `animateCords` do esboço);
  - leituras de posição todas antes das escritas no SVG, para não forçar layout repetido.
  - Curva: `M x1 y1 C x1+d y1, x2-d y2, x2 y2` com `d` proporcional à distância horizontal.
- **Grade de exemplares**: um *Web Component* (`<elz-item-grid>`) em JS puro, com `tabindex` itinerante; as linhas visíveis são renderizadas e as demais virtualizadas acima de ~200 exemplares.
- **Orçamento**: JS da tela de acervo < 30 KB comprimido, sem dependências além do HTMX já vendorizado; CSS < 25 KB. Medido no CI (como o tempo de rebuild do Askama, doc 04).

**Dados**: a linha geral pede ao SQLite uma consulta por página para disponibilidade; o resto da família vem do índice e do cache de fragmentos. Foco carrega a família inteira numa leitura do pool.

**Esboço v9** (decisões aprovadas):

- A animação de `grid-template-columns` roda só na família focada; as outras linhas não mudam de layout.
- O autocomplete de autoridades consulta o índice (prefixo por classe), nunca uma lista carregada no navegador.
- Cordinhas só dentro da família. Uma autoridade ligada a muitos registros mostra a contagem ("+2.998 obras") em vez de desenhar um fio por registro.
- Nada de `onclick` ou `<script>` inline (a CSP proíbe): o comportamento vem de listeners delegados num módulo JS da tela.

## 12. Acessibilidade, idioma e privacidade

- Listagem como `role="feed"` de `article`s; cada bloco é focável e tem nome acessível ("Obra: Um estudo em vermelho, Doyle"). As cordinhas são decorativas (`aria-hidden`); a relação é dita no nome do bloco ("Instância, edição de …").
- Semáforo com ícone e texto, nunca só cor; contraste AA nos dois temas.
- Toda ação de arrastar tem alternativa por teclado (§7.1); a grade segue o padrão de *grid* do WAI-ARIA.
- Textos só pelo Fluent (inglês primeiro, depois pt-BR), incluindo rótulos das cordinhas e nomes dos campos dos perfis.
- **LGPD** (visão, A.2): nada de leitor nas linhas, pílulas ou cordinhas; os dados do leitor só existem do outro lado do muro (§18). Na grade, o empréstimo mostra só "emprestado até 20/11"; quem emprestou aparece só com permissão de circulação e por ação explícita. O OPAC usa a mesma anatomia de família **sem** edição, grade, quarentena nem dados de circulação além de disponível/indisponível.

## 13. Atalhos de teclado

| Atalho | Ação |
|---|---|
| `/` | Ir para a busca |
| `↑` / `↓` | Família anterior / seguinte |
| `←` / `→` | Bloco à esquerda / à direita na família (Entidades ↔ Hub ↔ Obra ↔ Instância ↔ Exemplares) |
| `Enter` | Focar o bloco (80/20) / abrir a grade a partir de um exemplar |
| `Esc` | Fechar o que é passageiro (célula, minicaixinha, estação, diálogo); sem nada aberto, voltar à visão geral; no lado do leitor, voltar ao acervo (§3) |
| `E` ou `F2` | Editar o bloco focado |
| `Ctrl+Enter` | Concluir (salvar) o bloco ou o lote da grade |
| `Ctrl+N` | Novo bloco na coluna focada; sem foco, família nova (catalogação contínua) |
| `G` então `H`/`W`/`I`/`X` | Ir para Hub / Obra / Instância / Exemplares da família |
| `M` | Modo mesclar (alternativa ao arraste) |
| `Ctrl+D` | Na grade, copiar a célula de cima |
| `Alt+↑` / `Alt+↓` | Reordenar o chip ou a linha selecionada |
| `?` | Mostrar os atalhos |

Atalhos de letra só valem fora de campos de texto. Nenhum atalho conflita com os do navegador mais comuns (`Ctrl+N` só é capturado com o foco dentro da lista).

## 14. Fases e critérios de aceite

| Etapa | Entra | Depende de | Aceite |
|---|---|---|---|
| Onda 1a: lista e busca | Linha-família só leitura, semáforo (verde/vermelho/cinza), busca unificada, foco 80/20 com cordinhas e trilha | 1.8 (Tantivy), 1.9 | 1 milhão de registros: p95 < 50 ms por página de busca no servidor; transição de foco sem quadro perdido em Chrome e Firefox num PC de balcão de referência; navegação completa só por teclado |
| Onda 1b: edição | Edição no lugar, perfis livro e periódico, `update_work`/`update_instance` com `row_version`, catalogação contínua | 1.6, lacunas de §10 que o perfil usar | Editar e exportar: o MARC muda só nos campos editados (teste de ida e volta) |
| Onda 1c: grade | `<elz-item-grid>`, bipe, lote, `save_items` | 1.5 | 50 exemplares bipados seguidos sem perder leitura; recusa por célula |
| Onda 1d: derivada | Rascunhos por ISBN da própria base; estação Z39.50/SRU (§17) na Fase 4 | 1.7, 1.10 (matching), Fase 4 | ISBN bipado de livro já existente leva à grade dele |
| Onda 1e: autoridades | Coluna de Autoridades, "+ Autoridade" com autocomplete, ligação por papel (§16), aviso de alcance ao renomear | `agent_identifiers`, `update_agent` | Renomear um agente reflete em todas as páginas de registro e no MARC exportado |
| Estação de IA | Estação de IA (§17.2), ligada pela configuração quando a porta de fontes de rascunho existe | porta `DraftSource`, Onda 1d | Nenhum campo gravado sem aprovação; ISBN com dígito inválido recusado; cada campo com a marca de origem |
| Circulação no muro | Minicaixinha de empréstimo, lado do leitor, cordinhas laranja (§18) | Fase 3 (circulação, CalcDateDue) | Data de devolução igual à do Koha; recusas na minicaixinha; dados do leitor ausentes do HTML do acervo |
| Onda 2: grafo e mesclagem | Arraste de cordinha, decisão, quarentena, desfazer; canvas horizontal completo | Onda 1, `merge` | Mesclar e desfazer deixam banco e MARC exportado como antes |

## 15. O que muda em relação ao esboço

Preservado: listagem vertical contínua, família BIBFRAME da esquerda para a direita com cordinhas Bézier, Hub que colapsa, pílulas com semáforo e "+N", foco 80/20 com contexto ligado, expansão vertical com campos do perfil, grade de exemplares ancorada à Instância, escurecimento das irmãs, trilha superior, "+ Nova" por coluna, arraste para mesclar com as duas saídas.

Refinado ou acrescentado:

1. **Estado na URL** (voltar, recarregar, mandar link ao colega).
2. **Quarentena como padrão** na mesclagem, com contagem do que move e **desfazer**.
3. **Alternativa por teclado** para o arraste e atalhos para toda a navegação.
4. **Semáforo com mais dois estados** (em trânsito/reservado, não emprestável) e sem depender só de cor.
5. **Salvar por bloco e por lote**, nunca por tecla; recusas no próprio campo.
6. **Catalogação derivada e duplicatas** no primeiro campo da família nova.
7. **Capa pelo servidor**, com cache, sem o navegador chamar terceiros.
8. **OPAC** reaproveita a anatomia, sem edição nem dados internos.
9. O esboço mostra contribuidor da edição e etiquetas, que o modelo ainda não tem: ficam em §10, para entrar com migração e mapeamento antes da tela.

## 16. Coluna de Autoridades e ligação por papel

A primeira coluna da família deixa de ser só leitura (esboço v9): é a coluna de **Autoridades**, com as entidades compartilhadas que o núcleo já trata como tabelas próprias (`agents`, `subjects`).

### 16.1 Criar e buscar

- Botão **"+ Autoridade"** no topo da coluna, com seletor de classe e busca instantânea:

| Classe na tela | BIBFRAME | Modelo |
|---|---|---|
| Pessoa | `bf:Person` | `agents`, `AgentKind::Person` |
| Família | `bf:Family` | `agents`, `AgentKind::Family` |
| Instituição | `bf:Organization` | `agents`, `AgentKind::Organization` |
| Evento | `bf:Meeting` | `agents`, `AgentKind::Meeting` |
| Assunto (tópico, gênero/forma, período) | `bf:Topic`, `bf:GenreForm`, `bf:Temporal` | `subjects`, `SubjectKind` correspondente |
| Lugar | `bf:Place` | `subjects`, `SubjectKind::Place` |

- O autocomplete consulta o índice por prefixo na classe escolhida; o último item é sempre "Criar … 'texto digitado'", explícito, como em §4.1 item 6.
- **Série não é autoridade desta coluna**: uma série ("Coleção Vaga-Lume") é uma Obra ou um Hub ao qual a Obra se liga por `work_relations` (`PartOf`), criada pela coluna Obra. **Coleção local** ("Acervo Histórico de Palotina") é atributo do exemplar (vocabulário `Collection`, 952 `$8` no Koha) e se aplica na grade (§5), nunca com cordinha.

### 16.2 Ligar por papel

A validação de onde uma cordinha pode cair é por **papel da ligação**, não pela classe da autoridade. A mesma Pessoa pode ser autora de uma Obra e assunto de outra (um livro sobre Doyle); um Evento pode ser autor de anais; um Lugar pode ser assunto ou local de publicação.

| Papel | Autoridades aceitas | Destinos válidos | Modelo |
|---|---|---|---|
| Contribuição (autor, ilustrador, tradutor…) | Pessoa, Família, Instituição, Evento | Obra; Instância (contribuidor da edição); Hub (criador) | `contributions` com `relator`; `instance_contributions` (§10) |
| Assunto | todas | Obra | `work_subjects` (agente como assunto: `SubjectKind::Agent`) |
| Local de publicação | Lugar | Instância | `instances.place_id` (§10) |

- Ao começar a arrastar, o papel padrão vem da classe (agentes: contribuição; assuntos e lugares: assunto), e só os destinos válidos para **algum** papel se iluminam. Ao soltar num destino que aceita mais de um papel, aparece uma escolha curta de papel (lista com setas, `Enter` confirma), já marcada no padrão; o papel de contribuição pede o termo relator (autocomplete em `terms`, vocabulário `Relator`).
- A regra vive no comando (`catalog.link_authority { authority, target, role, relator }`), não no JavaScript: a tela só ilumina o que o servidor mandou como válido para aquela classe e mostra a recusa se o comando discordar.

### 16.3 Editar e renomear

- Editar uma autoridade muda **todos** os registros ligados. O bloco focado mostra o alcance antes de salvar ("afeta 3.012 registros") e o comando (`catalog.update_agent`, `catalog.update_subject`) recusa com `row_version` velho, como os demais.
- O mesmo comando, na mesma transação, **sobe o `row_version` de todas as Instâncias ligadas** e grava um efeito de reindexação para cada uma. Sem isso, o cache de páginas do OPAC por versão do registro (spec `docs/specs/opac/busca-e-registro.md`, item 9) mostraria o nome antigo. A reindexação vai em lotes pelo indexador (tarefa 1.8), sem travar a escritora.
- Mesclar autoridades segue §7 (Agente → Agente), e o cabeçalho de origem vira forma variante.
- Identificadores externos (VIAF, LCNAF, BN) ficam em `agent_identifiers` (§10) e aparecem no bloco focado como chips com link para a fonte.

## 17. Estações de captura no bloco: Z39.50/SRU e IA

Um bloco focado de Autoridade, Obra ou Instância pode trocar o seu corpo por uma **estação de captura**: os campos normais somem enquanto ela está aberta, para não poluir a tela, e voltam ao fechar (botão "← Voltar" ou `Esc`). Nada de modal nem de bloco solto.

### 17.1 Porta comum: fontes de rascunho

- Z39.50, SRU e IA são **fontes de rascunho** do doc 04 atrás de uma só porta da camada de aplicação (`DraftSource`): recebem uma consulta (texto, título/autor/ISBN, fotos) e devolvem rascunhos com a origem. Rodam **só no servidor**; o navegador nunca fala com servidores externos (CSP, privacidade, funcionamento sem internet no balcão).
- Cada fonte tem tempo-limite próprio; as fontes configuradas são consultadas em paralelo e os resultados chegam à estação conforme ficam prontos (HTMX com polling curto ou SSE). Uma fonte lenta ou fora do ar aparece como "sem resposta", sem segurar as outras.
- Configuração no `elzevir.toml`, por instituição: servidores com host, porta, base, sintaxe e codificação, em ordem de prioridade. O botão "⚙️ Servidores" mostra e liga/desliga os da instituição; editar a lista é tarefa de administração.
- **Aprovar** (`✓ Aprovar`) nunca grava direto:
  - autoridade: preenche o bloco da Autoridade com o nome autorizado, datas e o identificador da fonte (`agent_identifiers`);
  - registro bibliográfico: o MARC recebido passa por `marc_to_bibframe` (mesmo caminho da importação) e pelo **matching** da tarefa 1.10, que propõe pendurar a Instância numa Obra existente (outra edição) em vez de criar Obra nova. Duplicata de ISBN leva à grade da Instância existente (§6.3 item 5). O MARC recebido vira o registro de origem, com `origin = z3950:<servidor>` ou `sru:<servidor>` (§10).
- Contexto por bloco: na Autoridade, busca nas bases de autoridade (BN, VIAF, LCNAF); na Obra e na Instância, busca bibliográfica por título, autor e ISBN.

### 17.2 Estação de IA (multimodal)

Especificada agora para ser ligada assim que a porta de fontes de rascunho existir (decisão de Paulo, 2026-11-13). Entra desligada por padrão.

1. **Entradas**: fotos (capa, folha de rosto, ficha catalográfica) e um prompt opcional para aquele exemplar.
2. **Ordem do pipeline**, para reduzir invenção:
   1. ler a ficha catalográfica (CIP) e a folha de rosto;
   2. extrair o ISBN e validar o dígito verificador; ISBN inválido é descartado e marcado;
   3. buscar pelo ISBN na própria base (duplicata) e nas fontes Z39.50/SRU configuradas;
   4. só então pedir ao modelo que preencha as **lacunas** que nem a ficha nem as fontes cobriram.
3. **Saída em esquema fixo**: o modelo devolve JSON validado contra um esquema do Elzevir (campos de Obra e Instância do perfil da categoria), que vira um rascunho igual aos outros. O texto de instrução da biblioteca complementa o pedido, mas não troca o formato; saída fora do esquema é descartada com aviso.
4. **Marca de origem por campo**: cada valor do rascunho mostra **"lido da foto"**, **"da fonte <servidor>"** ou **"sugerido"**. CDD, Cutter e assuntos sugeridos aparecem como sugestão, nunca como valor pronto. A marca vai para o `audit_log` quando o rascunho é aprovado; o registro de origem leva `origin = ai:<modelo>`.
5. **Nenhuma gravação sem aprovação** humana, campo a campo editável antes de aprovar.
6. **Configuração** no `elzevir.toml`, por instituição:
   - provedor e modelo como **texto livre** (sem lista fixa no código, que envelhece);
   - chave de API só no servidor (variável de ambiente ou arquivo de segredo referenciado), nunca enviada ao navegador nem gravada no banco;
   - suporte a modelo local (Ollama) para quem não quer mandar fotos para fora;
   - padrão descritivo e de classificação (RDA ou AACR2, CDD com a edição, tabela Cutter da biblioteca, como PHA) e a instrução permanente da biblioteca.
7. **Fotos**: usadas na chamada e descartadas, a não ser a capa, que pode virar a capa da Instância (§6.4) se o catalogador aprovar.

## 18. Muro Acervo ↔ Leitores e empréstimo

O acervo e o leitor ficam em lados opostos de um **muro** visual na borda direita do acervo. O muro existe **no servidor**, não só no CSS.

### 18.1 Regras do muro

- **Lado do acervo**: a mesma tela de sempre (§2 a §8), sem nenhum dado de leitor no HTML.
- **Lado do leitor**: rota própria, `/equipe/leitores/{id}`, carregada só ao atravessar o muro, com **permissão de circulação**; cada abertura da ficha fica no `audit_log` (quem viu qual leitor e quando). Ao voltar, o lado do leitor sai do DOM; dado de leitor não fica escondido fora da tela.
- A animação de deslizar entre os lados é só transição (View Transitions); o que muda de verdade é a rota e o fragmento carregado.

### 18.2 Fluxo 1: emprestar a partir do exemplar

1. Pílula verde (disponível) mostra o botão **"⇄ Emprestar"** para quem tem permissão de circulação.
2. O botão abre uma **minicaixinha** na própria pílula com o campo do `cardnumber` (leitor de código de barras ou digitação); `Enter` envia.
3. O servidor aplica as **regras de circulação** (Fase 3): data de devolução calculada com paridade com o Koha (CalcDateDue), bloqueios, débito acima do limite, reserva de outro leitor. A data vem da resposta, nunca do navegador.
4. Sucesso: a pílula vira amarela com **"Dev. DD/MM"** e uma seta **→** encostada no muro. Recusa: o motivo aparece dentro da minicaixinha (por exemplo, "Débito de R$ 12,00 acima do limite"), e a pílula continua verde.
5. A seta → só aparece com permissão de circulação e atravessa o muro para a ficha daquele leitor. O aviso de sucesso no acervo mostra o leitor só para quem tem essa permissão.

### 18.3 Fluxo 2: a partir do leitor

1. Do outro lado do muro, a **ficha do leitor** fica à direita, com a caixa de empréstimo por código de barras **dentro do próprio bloco do leitor**.
2. À esquerda, só os exemplares **emprestados a esse leitor**, ligados à ficha por cordinhas laranja, nada mais do acervo.
3. Cada bipe envia o empréstimo pelas mesmas regras do fluxo 1; no sucesso, o exemplar entra na lista e a cordinha nova se desenha animada; na recusa, o motivo aparece na caixa.
4. Renovar e devolver ficam em cada exemplar da lista, também pelos comandos de circulação.

### 18.4 Ficha do leitor e LGPD

- CPF mostrado **mascarado** (`***.419.829-**`); o completo só por ação explícita, registrada no `audit_log`.
- **Nenhum campo de perfil de leitura** (nada como "retira romances policiais quinzenalmente"): hábito de leitura é dado sensível.
- Histórico de empréstimos só com **consentimento** do leitor, como a preferência de privacidade do Koha (`privacy`).
- Fixtures e esboços usam leitores fictícios (`CLAUDE.md`).

## 19. Decisões do esboço v9

Aprovadas por Paulo em 2026-11-13, a partir da análise do esboço `elzevir-esboco-v9.html`:

1. **Coluna de Autoridades** à esquerda, com "+ Autoridade", seletor de classe e autocomplete pelo índice (§16.1).
2. **Validação por papel**, não por classe: contribuição, assunto e local de publicação, cada um com seus destinos (§16.2).
3. **Série** é relação entre Obras (`PartOf`); **coleção local** é atributo do exemplar, aplicado na grade (§16.1).
4. **Identificador externo de autoridade** em `agent_identifiers`, junto das formas variantes (§10).
5. **Renomear autoridade** avisa o alcance e sobe a versão de todas as Instâncias ligadas, por causa do cache por versão do OPAC (§16.3).
6. **Estações de captura dentro do bloco** (Z39.50/SRU e IA), no servidor, atrás da porta de fontes de rascunho, com tempo-limite por fonte e aprovação pelo matching da 1.10 (§17.1).
7. **IA** com pipeline híbrido (CIP, ISBN validado, fontes, só depois lacunas), esquema fixo, marca "lido da foto" / "da fonte" / "sugerido", sem gravação sem aprovação, chave e modelo em texto livre no `elzevir.toml`, Ollama local. A estação fica especificada junto da captura derivada para ser ligada quando a porta existir (§17.2).
8. **Muro no servidor**: rota própria do leitor, permissão de circulação, `audit_log` de cada acesso, nada de leitor no DOM do acervo (§18.1).
9. **Empréstimo inline** pelas regras de circulação do servidor, com recusas na minicaixinha e seta → só com permissão (§18.2).
10. **Ficha do leitor**: CPF mascarado, sem perfil de leitura, histórico só com consentimento (§18.4).
11. **`Esc` em camadas**: fecha o passageiro primeiro e depois volta direto à visão geral; edição pendente nunca é descartada em silêncio (§3).
12. **Desempenho**: animação só na família focada, autocomplete pelo índice, cordinhas só dentro da família com contagem para autoridades muito ligadas, sem JS inline (§11).

## 20. Mecânicas pelo olhar do catalogador

Aprovadas por Paulo em 2026-11-14, com o rascunho `elzevir-rascunho-v10.html` como referência visual. Valem sobre o que estiver em conflito nas seções anteriores. O critério é o trabalho do dia a dia, nesta ordem de frequência: acrescentar exemplar a uma Instância que já existe, catalogar nova edição de uma Obra que já existe e, por último, criar um registro novo.

1. **Um campo para tudo.** A busca da barra aceita bipe de ISBN, código de barras ou texto e decide sozinha: código de barras conhecido abre a grade daquele exemplar; ISBN conhecido abre a grade da Instância; ISBN desconhecido abre "+ Nova" já com o rascunho (§6.3); o resto é busca. A regra fica no servidor, numa função só.
2. **Lista compacta, blocos no foco.** Fora do foco, cada família é uma linha compacta (título, contribuidor principal, classificação, edições e exemplares com o sinal), de modo que caibam umas 15 por tela. A família focada abre os blocos com cordas (§2, §3). A grade de cinco trilhas e as cordas valem só para a família focada.
3. **Cordas para entender, teclado para ligar.** Ligar agente, assunto, Hub ou série é digitar no campo com autocompletar (§4.1 item 6). Arrastar fica só para mover exemplar e mesclar (§7), sempre com "Desfazer".
4. **O bloco é a unidade de salvar.** Não existe "salvar registro". Cada bloco salva sozinho com `row_version` (§9). Depois de salvar aparece um aviso com "Desfazer", que grava os valores anteriores como uma nova versão. Diálogo de confirmação só na mesclagem (§7.2).
5. **Hub sugerido, não exigido.** Ninguém cria Hub ao catalogar. Quando uma segunda Obra recebe o mesmo título uniforme de outra, o bloco sugere "Agrupar num Hub", e aceitar cria o Hub e as duas ligações num comando.
6. **Autoridades como chips fora do foco.** Na linha compacta, autor e assuntos são chips dentro da Obra. Os cartões de autoridade e as cordas tracejadas aparecem só na família focada.
7. **Catalogação em série herda o anterior.** No `Ctrl+N` (§6.2), editora, local, ano, idioma e assuntos vêm do registro anterior, marcados como "herdado" até serem confirmados ou trocados.
8. **Listas de trabalho e facetas numa faixa horizontal.** Logo abaixo da busca, uma faixa compacta tem os filtros salvos ("todo o acervo", "meus recentes", "incompletos para o perfil", "importados hoje", "em quarentena") como botões segmentados, as facetas (material, unidade, idioma, ano) como menus com contagem, os filtros ativos como chips removíveis e a ordem. Não há barra lateral: a largura inteira fica para os blocos. O catalogador trabalha em lote.
12. **Largura das colunas.** Obra e Instância são as colunas mais largas, porque têm os campos que o catalogador lê; Autoridades e Hub são as mais estreitas (proporção de referência 0,8 : 0,7 : 1,55 : 1,7 : 1,35). Ocultar uma coluna devolve a largura dela às outras.
9. **Validar sem travar.** Campo opcional vazio nunca impede salvar. Faltar campo obrigatório do perfil marca o registro como "incompleto", que entra na lista de trabalho; só o mínimo para existir (título da Obra, título da Instância, tipo de material) é recusa no comando.
10. **Todos os campos BIBFRAME.** O bloco focado tem "Todos os campos", que mostra, além do perfil da categoria (§4.2), todas as propriedades que o modelo guarda para aquela entidade, agrupadas (títulos variantes, identificadores, notas, relações, papéis, datas, público), ainda sem etiqueta MARC. Propriedade do BIBFRAME que o modelo não guarda entra pela lista de lacunas (§10), com migração e mapeamento antes da tela.
11. **Exportar o registro.** O bloco focado da Instância exporta o registro em MARC21 (ISO 2709) e MARCXML, pela mesma `bibframe_to_marc` da exportação em lote, e em BIBFRAME 2.0 RDF/XML, no vocabulário da Library of Congress. Formatos de citação e intercâmbio (RIS, BibTeX, MODS, Dublin Core) vêm com a interoperabilidade (Fase 4). A seleção de várias famílias exporta em lote no mesmo formato.

| Mecânica | Tarefa |
|---|---|
| 2, 6 (lista compacta, chips) | 2.4, com o foco 80/20 |
| 3, 4, 9, 10 (ligar digitando, salvar por bloco com desfazer, incompleto, todos os campos) | 2.5 |
| 5, 7 (Hub sugerido, herança no `Ctrl+N`) | 2.6 |
| 1 (um campo para tudo) | 2.7 (código de barras) e 2.8 (ISBN) |
| 8 (listas de trabalho e facetas na faixa horizontal) | 2.10 (nova) |
| 12 (largura das colunas) | 2.3 (feito) |
| 11 (exportar o registro) | MARC21 e MARCXML na 2.4; BIBFRAME RDF/XML na 2.11 (nova); citação e intercâmbio na Fase 4 |
