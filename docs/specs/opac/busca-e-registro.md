# OPAC: busca e registro (v0)

> Status: v0, tarefa 1.9. Base: [doc 03](../../arquitetura/03-estrategia-reconstrucao.md) §1.2 (item 4) e §2.3, [visão de arquitetura](../../ARCHITECTURE_VISION.md) B.5, [interface](../../UI_BIBFRAME_INTERACTION_SPEC.md) §8 e [busca](../catalogo/busca.md).

## 1. Comportamento

### Busca: `GET /search?q=&page=`

1. O texto é aparado e cortado em **256 caracteres**; o resto é ignorado. Texto vazio mostra a página sem lista.
2. **20 resultados por página.** `page` vale de 1 a **500**: abaixo de 1 vira 1, acima de 500 vira 500, e o que não é número vira 1. O limite impede que um endereço forjado faça o índice pular milhões de documentos.
3. Cada resultado mostra título, autor principal, data, categoria de material, número de chamada e disponibilidade. Os quatro primeiros vêm guardados no índice, sem consulta ao SQLite ([busca](../catalogo/busca.md), item 4).
4. A **disponibilidade da página inteira sai de uma só consulta** ao SQLite: `CatalogReader::item_statuses` lê o estado de todos os exemplares das Instâncias da página. Um exemplar está disponível quando não está indisponível para empréstimo, danificado, perdido, retirado nem restrito (`ItemStatus::is_available`). Empréstimos entram na Fase 3. A linha diz "N de M disponíveis", "Nenhum exemplar disponível" ou "Sem exemplares".
5. Links "Anterior" e "Próxima" levam o texto codificado para URL. "Próxima" só aparece se há mais resultados e a página seguinte não passa de 500.
6. **HTMX:** o formulário pede `/search` com `hx-get`. Com o cabeçalho `HX-Request`, a resposta é só a lista (`<section id="results">`), que substitui a anterior. Sem ele, a página inteira. A URL vai para o histórico (`hx-push-url`), então voltar e recarregar funcionam sem JavaScript.

### Registro: `GET /record/{id}`

7. Mostra título, título da Obra (se diferente), autores, categoria, publicação (editora e data), classificação, assuntos, identificadores, idiomas, resumo e a tabela de exemplares: biblioteca, localização, número de chamada e estado. O estado mostrado é o mais grave marcado (perdido, retirado, danificado, restrito, indisponível para empréstimo), senão "Disponível".
8. `id` que não é número, ou Instância que não existe, dá **404** com a página "Registro não encontrado".
9. **Fragmento em cache por versão.** O corpo do registro é renderizado uma vez por **versão do registro** (`instances.row_version`) e guardado em memória. Cada pedido lê só a versão (uma consulta pela chave primária); se a versão guardada é a mesma, devolve o corpo guardado sem ler o registro. A versão sobe quando a Instância muda e quando um exemplar é acrescentado (`AddItem`); comandos futuros que mudem a Obra ou os exemplares precisam subir a versão também.
10. A versão é lida **antes** do registro. Se o registro muda entre as duas leituras, o corpo novo fica guardado sob a versão velha e é renderizado de novo no pedido seguinte: nunca se guarda um corpo velho sob uma versão nova.
11. O cache guarda até **10.000 registros**; ao encher, recomeça vazio. Um corpo tem poucos kB, então o cache fica em dezenas de MB. Há um idioma por servidor por enquanto; com mais de um, o idioma entra na chave.

### Erros e limites

12. Falha do índice ou do catálogo responde **503** com texto curto, sem detalhes para o visitante.
13. O OPAC lê só `SearchIndex` e `CatalogReader`. Nenhum dos dois chega a leitores ou empréstimos (visão B.5, LGPD).
14. As leituras rodam fora das threads do servidor HTTP (`spawn_blocking`), para que SQLite e Tantivy não travem o atendimento de outros pedidos.

### Indexador no servidor

15. `elzevir serve` abre o índice em `<data-dir>/index` e mantém uma thread, `elzevir-indexer`, que chama `index_pending` **a cada segundo**. Um registro catalogado aparece na busca em até cerca de um segundo. Falha é escrita no terminal e tentada de novo no próximo ciclo; como o cursor só anda com commit, nada se perde. `ELZEVIR_SERIAL=index` vale aqui como na [busca](../catalogo/busca.md), item 10.

## 2. Origem no Koha

`opac-search.pl` e `opac-detail.pl`. O Koha calcula a disponibilidade dos resultados exemplar por exemplar a partir do MARC guardado no índice (`searchResults` em `C4::Search`) e não guarda a página do registro. O Elzevir lê os estados da página numa consulta e guarda o corpo do registro por versão. As regras de "disponível" seguem as do Koha (`notforloan`, `damaged`, `itemlost`, `withdrawn`, `restricted`), sem empréstimos por enquanto.

## 3. Paridade

`livre` (OPAC e busca são áreas de liberdade). Prova:

- `crates/elzevir-web/tests/opac.rs` (catálogo e índice falsos): disponibilidade e título escapado na lista; HTMX recebe só a lista; páginas com o texto codificado e `page` inválido; texto vazio; registro com exemplares e estados, renderizado uma vez por versão e de novo quando a versão muda; 404 para id inexistente, não numérico ou negativo.
- `crates/elzevir/tests/opac.rs`, sobre os 200 registros da base de testes, com o SQLite e o Tantivy de verdade: buscar, abrir o registro, acrescentar um exemplar perdido e ver a página do registro mudar.
- `crates/elzevir-store/tests/catalog.rs`: `AddItem` sobe a versão; `item_statuses` devolve os estados de várias Instâncias numa chamada.
- `crates/elzevir-web/src/opac.rs`: toda categoria de material e todo estado têm texto no Fluent.

## 4. Desempenho

Critério do doc 03: p95 abaixo de **50 ms na busca** e de **30 ms no registro**, sobre **1 milhão de registros**.

Medida com `cargo bench -p elzevir --bench opac` (`ELZEVIR_BENCH_RECORDS` muda o tamanho): o arquivo de 200 registros repetido até 1 milhão, sem códigos de barras para as cópias não colidirem. Cada consulta de palavras acha milhares de registros, o caso difícil para ordenar e contar. Os pedidos passam pelo roteador inteiro, renderização incluída: 2.000 buscas (as 50 consultas de referência nas páginas 1 a 3) e 2.000 registros espalhados pelo acervo, primeiro sem cache e depois com cache.

Medida de 2026-11-13, no contêiner de desenvolvimento (4 núcleos, 15 GB, build `release`, banco e índice em disco):

| Pedido | p50 | p95 | p99 | máximo | Critério |
|---|---|---|---|---|---|
| Busca | 0,90 ms | **4,56 ms** | 5,98 ms | 11,39 ms | p95 < 50 ms |
| Registro, primeira vez | 0,17 ms | **0,31 ms** | 0,41 ms | 1,29 ms | p95 < 30 ms |
| Registro, do cache | 0,04 ms | **0,06 ms** | 0,08 ms | 0,14 ms | p95 < 30 ms |

Montar o acervo levou 399 s de importação (cerca de 2.500 registros/s com o banco já grande) e 40 s de indexação. Com 20 mil registros, a busca dá p95 de 0,61 ms: o custo cresce com o número de resultados que casam, não com o tamanho do acervo em si. O critério passa com folga de 10 vezes; não houve otimização além do desenho (lista lida do índice, disponibilidade numa consulta, corpo do registro em cache).

## 5. Fora desta versão

- Facetas, filtros e ordenação diferente da relevância.
- Disponibilidade com empréstimos e reservas (Fase 3).
- Nomes de biblioteca e localização: a tabela mostra os códigos até existir o cadastro com descrição na interface.
- Notas (500, 505, 590) na página do registro, que dependem de notas no modelo ([busca](../catalogo/busca.md), §4).
- Português na interface: os textos estão só em inglês, como o resto da interface por enquanto.
