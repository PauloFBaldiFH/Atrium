# Busca no acervo: índice e indexador (v0)

> Status: v0, tarefa 1.8, parte 1. Base: [doc 03](../../arquitetura/03-estrategia-reconstrucao.md) §1.2 (itens 3 e 8) e §2.3, [visão de arquitetura](../../ARCHITECTURE_VISION.md) B.5, [interface](../../UI_BIBFRAME_INTERACTION_SPEC.md) §8 e [importação](importacao.md). A parte 2 compara 50 consultas de referência com o Koha (precisa da instância de testes).

## 1. Comportamento

### Documento

1. **Um documento por Instância**, montado do modelo (`document_of`), nunca do MARC (visão A.1). O doc 03 pedia extração gerada do `mappings.yaml` do Koha; esse arquivo descreve etiquetas MARC, e a visão aprovada depois tirou o MARC do núcleo. Os nomes e o papel dos campos seguem os do Koha, mas a extração lê `InstanceView` e `ItemView`.
2. Campos de palavras (sem acento e sem diferença de maiúsculas: `coração` acha `Coracao`): título da Instância, título da Obra, contribuidores, assuntos e editora. Título conta o dobro.
3. Campos de código, comparados inteiros: identificadores (ISBN e ISSN sem hífen nem espaço, `X` maiúsculo), código de barras, tombo, número de chamada (sem diferença de maiúsculas), classificação, localização e idioma.
4. Campos guardados para montar a lista sem ir ao SQLite (B.5): Instância, Obra, Hub, título, autor principal (o primeiro contribuidor), data, categoria (a da Instância, senão a da Obra), classificação e números de chamada. A disponibilidade não entra no índice: vem do SQLite numa consulta por página (tarefa 1.9).

### Consulta

5. O texto digitado vale ao mesmo tempo como:
   - palavras: todas precisam aparecer (E), em qualquer campo de palavras. Sintaxe que o leitor de consultas não entende é ignorada, nunca vira erro;
   - código exato: ISBN (com ou sem hífen), código de barras, tombo, número de chamada ou classificação. Código exato vale 10 vezes uma palavra, então aparece primeiro;
   - prefixo de classificação: `82*` ou `869.3*` (algarismos e pontos antes de um `*` final).
6. Resultados por relevância; empate pela Instância de menor número. Até `limit` resultados.

### Indexador

7. `index_pending` lê `effect_outbox` em ordem a partir do **cursor** salvo no índice, em lotes (1.024 efeitos por padrão). Cada `catalog.reindex_instance` refaz o documento daquela Instância; Instância repetida no lote é lida uma vez; Instância que não existe mais tem o documento removido. Outros efeitos são pulados.
8. Cada lote é confirmado no Tantivy **junto com o cursor** (carga útil do commit): depois de uma queda, o índice tem o lote e o cursor, ou nenhum dos dois. Rodar de novo não refaz nada.
9. O índice tem um único escritor (`IndexWriter` atrás de um `Mutex`) e um único leitor compartilhado (`IndexReader`), recarregado depois de cada commit.
10. **`ELZEVIR_SERIAL=index`**: os documentos do lote são montados numa thread só, e o escritor do Tantivy usa uma thread de indexação (até 8 sem o interruptor).
11. `stored_documents()` devolve todos os documentos guardados, em JSON, ordenados: duas bases indexadas a partir dos mesmos efeitos têm a mesma lista, em série ou em paralelo.

## 2. Origem no Koha

O Koha indexa com Zebra ou Elasticsearch a partir do MARC (`Koha::SearchEngine::Elasticsearch::Indexer`, `admin/searchengine/elasticsearch/mappings.yaml`) e reindexa por fila (`background_jobs` com `update_elastic_index`). O Elzevir indexa o modelo, e a fila é a `effect_outbox`.

## 3. Paridade

`livre` (busca é área de liberdade). Prova:

- `crates/elzevir/tests/search.rs`, sobre os 200 registros da base de testes importados:
  - documentos iguais em série e em paralelo (1, 4 e 8 threads; lotes de 3, 7, 64 e 1.024 efeitos);
  - indexar de novo não acha nada, e o cursor é o último efeito;
  - palavras sem acento e sem diferença de maiúsculas; assunto e autor também acham;
  - código de barras, número de chamada, classificação e ISBN com ou sem hífen vêm primeiro;
  - `310*` só traz classificação que começa com `310`;
  - índice em disco: documentos e cursor sobrevivem a fechar e abrir.
- `elzevir-app::search` (unidade): lotes seguem a outbox, Instância repetida vira um documento, Instância apagada vira remoção.

## 4. Casos de borda

1. Consulta vazia ou só com sintaxe inválida: nenhum resultado (ou só os códigos exatos), sem erro.
2. Índice novo: cursor 0.
3. Cursor salvo que não é número (índice corrompido): erro, não volta a 0 em silêncio.
4. Dois indexadores no mesmo diretório: o segundo não abre (trava do Tantivy).

## 5. Desvios

Nenhum (área livre).

## 6. Fora do escopo

- Consultas de referência contra o Koha: parte 2.
- Rodar o indexador sozinho no `elzevir serve` / `elzevir worker`: tarefas 1.9 e 1.11.
- Agrupar resultados por família (Hub ou Obra), facetas e paginação por cursor: tarefa 1.9 e Onda 1a.
- Radicalização (*stemming*) em português: quando as consultas de referência mostrarem que falta.
