# Modelo bibliográfico (v0)

> Status: v0, tarefa 1.5. Base: decisões 4 e 5 do [doc 04](../../arquitetura/04-decisoes.md) (modelo relacional inspirado em BIBFRAME 2.0, com o MARC original preservado), [ADR 0001](../../adr/0001-modelo-bibliografico.md), [ADR 0002](../../adr/0002-hub-relacoes-e-categoria.md) e a [visão de arquitetura](../../ARCHITECTURE_VISION.md). Esta versão define as entidades e as tabelas; o que cada campo MARC preenche é da tarefa 1.6.

## 1. Comportamento

1. **Obra** (`works`) é o conteúdo intelectual: título preferido (título uniforme), idiomas, contribuições (autores e colaboradores), assuntos, classificação e resumo.
2. **Instância** (`instances`) é uma edição publicada de uma Obra: título como está na publicação, edição, local, editora, data, descrição física, série e identificadores (ISBN, ISSN, LCCN, EAN). Toda Instância pertence a exatamente uma Obra.
3. Na v1, cada registro importado cria **uma Obra e uma Instância**. O esquema já aceita várias Instâncias por Obra, mas nada as funde automaticamente.
4. **Item** (`items`) é o exemplar físico de uma Instância: código de barras, unidade de origem e unidade atual, tipo de item, número de chamada, localização, coleção, situação e preços. É o que a circulação empresta.
5. **Agente** (`agents`) é uma pessoa, entidade coletiva, evento ou família, identificado pelo tipo e pelo cabeçalho. O mesmo cabeçalho do mesmo tipo é sempre o mesmo Agente.
6. **Assunto** (`subjects`) é identificado pelo tipo, pelo cabeçalho e pelo tesauro de origem.
7. **Registro de origem** (`source_records`): o MARC como chegou, byte a byte, um por Instância. A exportação de um registro não editado devolve esses bytes.
8. **Identidades numéricas**: tudo que é vocabulário é guardado como número, nunca como texto repetido.
   - Unidades (`branches`) e tipos de item (`item_types`) têm tabela própria, porque a circulação depende deles.
   - Os outros códigos ficam em `terms`, separados por vocabulário: idioma, função do colaborador (relator), localização, coleção, esquema de classificação e tesauro.
   - Tipos fixos (tipo de agente, de assunto, de identificador, formato do registro de origem) são `enum`s no domínio com um número estável cada.
9. A ordem importa e é guardada (`position`): idiomas, contribuições, assuntos e identificadores saem na ordem em que entraram.
10. Situação do exemplar (`not_for_loan`, `damaged`, `lost`, `withdrawn`, `restricted`): número, com 0 = normal. Os outros valores são definidos pela biblioteca, como os valores autorizados do Koha.
11. Dinheiro é inteiro em centavos. Datas são texto ISO 8601.
12. Propriedades pouco usadas vão para a coluna `extra` (JSON) da entidade, em vez de uma coluna para cada.
13. `works`, `instances`, `items`, `branches`, `item_types` e `hubs` têm `row_version` (doc 05, §3).
14. **Hub** (`hubs`) agrupa Obras de mesma origem (traduções, adaptações, título uniforme). É opcional: a Obra aponta para no máximo um Hub (`works.hub_id`).
15. **Relações entre Obras** (`work_relations`): tipo (`enum` estável: tradução de, adaptação de, parte de, suplemento de, continuação de) e a Obra relacionada, em ordem. Guardadas num sentido só: a linha fica na Obra que é a tradução, a adaptação, a parte.
16. **Categoria do material** (`material_type`, `enum` estável: livro, periódico, mapa, partitura, gravação sonora, vídeo, digital, material visual, misto) na Obra e na Instância. A Instância sem categoria própria recebe a da Obra; uma edição digital de um livro impresso é `livro` na Obra e `digital` na Instância. A categoria escolhe o ícone e o perfil de campos da catalogação.
17. **Tombo** (`items.inventory_number`, `952$i` no Koha): opcional e pode repetir, como o `stocknumber` do Koha.
18. Identificadores também incluem DOI e ISMN.

### Comandos e leitura

19. Toda escrita passa pela thread escritora como `WriteCommand::Catalog`: criar unidade, criar tipo de item, criar Hub, acrescentar registro (Obra + Instância, com categoria, Hub e relações) e acrescentar Item.
20. Os comandos citam vocabulário pelo **código** (`MAIN`, `BK`, `por`); a escritora troca pelo número.
    - Unidade e tipo de item precisam existir antes. Código desconhecido é recusado.
    - Os outros vocabulários (idioma, relator, localização, coleção, esquema, tesauro) são criados no primeiro uso, com o próprio código como rótulo.
21. Agente e Assunto já existentes são reaproveitados (itens 5 e 6), nunca duplicados.
22. Recusas, sem nada gravado: código de unidade ou tipo de item já usado, unidade ou tipo de item desconhecido, Hub, Obra relacionada ou Instância inexistente e código de barras repetido.
23. A leitura (`CatalogReader`) devolve uma Instância com sua Obra, Hub, categorias, relações, contribuições, assuntos, identificadores e idiomas na ordem gravada, e os Itens de uma Instância na ordem de criação.
24. Acrescentar registro ou Item grava, na mesma transação, o efeito `catalog.reindex_instance` (`{"instance": id}`) em `effect_outbox`, para o indexador (tarefa 1.8) refazer o documento daquela Instância depois do commit. Comando recusado não deixa efeito.

## 2. Origem no Koha

O modelo é do Elzevir, mas cada coluna tem de onde vir numa migração e para onde ir numa exportação. Tabelas em `reference/koha/installer/data/mysql/kohastructure.sql`:

| Elzevir | Koha |
|---|---|
| `works` + `instances` | `biblio`, `biblioitems` |
| `source_records` | `biblio_metadata` |
| `items` | `items` (`barcode` único, `stocknumber` → `inventory_number`, `homebranch`, `holdingbranch`, `itype`, `itemcallnumber`, `notforloan`, `damaged`, `itemlost`, `withdrawn`, `restricted`, `price`, `replacementprice`) |
| `hubs`, `work_relations`, `material_type` | não existem no Koha: Hub e relações vêm de `130`/`240`/`7XX` e a categoria do líder e do `008`/`007` (tarefa 1.6) |
| `branches` | `branches` |
| `item_types` | `itemtypes` |
| `terms` (localização, coleção) | `authorised_values` (`LOC`, `CCODE`) |
| `terms` (esquema de classificação) | `class_sources` |

O Koha não tem Agente nem Assunto como tabela bibliográfica: guarda `biblio.author` como texto e o resto só no MARC.

## 3. Paridade

`livre`: é modelo interno. A paridade exigida é a da **exportação MARC** (tarefas 1.6 e 1.11), que este modelo sustenta com o registro de origem preservado.

Testes: `crates/elzevir-store/src/schema.rs` (`catalog_tests`, esquema) e `crates/elzevir-store/tests/catalog.rs` (comandos e leitura).

## 4. Casos de borda

1. Apagar uma Obra que tem Instâncias, uma Instância que tem Itens, ou uma unidade ou tipo de item em uso: o banco recusa.
2. Apagar uma Instância sem Itens apaga junto os identificadores e o registro de origem dela.
3. Dois Itens com o mesmo código de barras: recusado. Vários Itens sem código de barras: aceito.
4. Mesmo código em vocabulários diferentes (por exemplo `REF` como localização e como coleção): são dois termos.
5. Assunto com o mesmo cabeçalho e tipo, um com tesauro e outro sem: são dois Assuntos. Dois sem tesauro: é o mesmo.
6. `extra` que não é JSON válido: recusado.
7. Número de tipo fixo desconhecido lido do banco (base de versão mais nova): erro, nunca um valor padrão.
8. Obra que se relaciona com ela mesma: recusado pelo banco. Apagar uma Obra que outras citam numa relação, ou um Hub com Obras: recusado. Apagar uma Obra leva junto as relações dela.
9. Registros gravados antes da migração 4 não têm categoria; a leitura devolve `None` para eles. Os comandos sempre gravam a categoria.

## 5. Desvios

Nenhum (paridade livre).

## 6. Fora do escopo

- Mapeamento MARC21 ↔ modelo, incluindo títulos variantes, notas e séries estruturadas: tarefa 1.6.
- Edição e exclusão de Obras, Instâncias e Itens pelos comandos: quando a catalogação na interface existir.
- Autoridades (registro de autoridade por Agente e Assunto), fusão de Obras e serialização BIBFRAME em RDF: fases seguintes.
- Empréstimos, reservas e leitores: Fase 3.
