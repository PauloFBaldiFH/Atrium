# Importação em lote de MARC21 (v0)

> Status: v0, tarefa 1.7. Base: [doc 03](../../arquitetura/03-estrategia-reconstrucao.md) §1.2 (itens 2, 3 e 8) e §2.2, [mapeamento MARC21](mapeamento-marc21.md) e [modelo bibliográfico](modelo-bibliografico.md). A linha de comando `elzevir import` é a tarefa 1.11; o casamento com registros já existentes, a 1.10.

## 1. Comportamento

`import_marc21(entrada, destino, opções)` lê um arquivo ISO 2709 e grava cada registro como Obra, Instância e Itens. Devolve um relatório: registros lidos, gravados, exemplares gravados e, em ordem de arquivo, cada registro que ficou de fora, com posição, deslocamento em bytes e motivo.

1. **Leitura em fluxo**: `MarcReader` com os limites de `Limits` (99.999 bytes, 5.000 campos). A memória não cresce com o arquivo: só um lote de registros fica em memória por vez.
2. **Pré-cálculo puro, em paralelo**: cada lote (1.024 registros por padrão) é dividido em fatias contíguas, uma por thread. Cada registro é convertido para UTF-8 (`marc21_to_utf8`, tarefa 1.4) e mapeado (`marc_to_bibframe`, tarefa 1.6). Nada aqui lê banco, relógio ou ambiente.
3. **Confirmação em série, na ordem do arquivo**: os registros mapeados entram na fila limitada da escritora (`AddRecord`), um a um, na ordem do arquivo. Quando a fila está cheia, a importação espera (não recusa). A escritora grava o lote anterior enquanto as threads mapeiam o seguinte.
4. **Registro de origem**: registro que já chegou em UTF-8 (líder 09 = `a` e bytes válidos) é guardado byte a byte. Registro convertido de MARC-8 ou Latin-1 é guardado já convertido, reescrito em ISO 2709.
5. **Falha de um registro não para a importação.** Ficam no relatório:
   - bytes que não são um registro ISO 2709 válido ou passam de um limite (`Unreadable`);
   - registro sem o que o modelo exige (`Mapping`: sem título, `952` sem unidade ou sem tipo);
   - registro recusado pela escritora (`Rejected`: unidade ou tipo de item desconhecido, código de barras repetido). Nada dele fica gravado.
6. **A importação para** só quando a entrada não pode ser lida (erro de E/S) ou a escritora parou. O que já foi confirmado fica.
7. **Efeitos**: cada registro gravado deixa um `catalog.reindex_instance` em `effect_outbox` (modelo bibliográfico, item 24).
8. **Interruptor `ELZEVIR_SERIAL`**: quando a lista (separada por vírgula, sem diferença de maiúsculas) contém `import`, o pré-cálculo roda numa thread só. Quem lê a variável é o binário; a camada de aplicação recebe `Parallelism`.
9. **Digest**: `Store::digest()` resume o conteúdo de todas as tabelas (ordenadas pelo nome e pelas colunas) num FNV-1a de 64 bits. Duas bases construídas pelos mesmos comandos na mesma ordem têm o mesmo digest.

## 2. Origem no Koha

O Koha importa por `tools/stage-marc-import.pl` e `tools/manage-marc-import.pl` (`C4::ImportBatch`), em duas etapas com tabelas de preparação. O Elzevir não copia essa estrutura: a etapa de preparação e o casamento entram na tarefa 1.10. Aqui só a leitura, o mapeamento e a gravação.

## 3. Paridade

`livre`. Prova (`crates/elzevir-store/tests/import.rs`):

- os 200 registros da base de testes entram inteiros (200 Instâncias, 253 exemplares, 200 registros de origem iguais aos bytes do arquivo, 200 efeitos);
- **digest igual** em série e em paralelo, com 4 e 16 threads e lotes de 3, 7, 64 e 1.024 registros;
- o digest muda quando um valor muda;
- registro quebrado no meio do arquivo: relatado com posição e deslocamento, os vizinhos entram;
- registros recusados pela escritora: relatados em ordem, sem deixar linhas nem efeitos.

## 4. Medidas

`cargo bench -p elzevir-store --bench import` importa os 200 registros repetidos 50 vezes (10.000 registros, sem `952$p` para não repetir código de barras). Numa máquina de 4 núcleos (2026-11-13):

| Medida | Resultado | Critério do doc 03 |
|---|---|---|
| Importação completa, em série | ~3.900 registros/s | > 2.000 registros/s |
| Importação completa, 4 threads | ~3.750 registros/s | |
| Só leitura e mapeamento, uma thread | ~78.000 registros/s | |
| Alocações na leitura | 1 no arquivo inteiro | |
| Alocações na leitura + mapeamento | 141 por registro (linha de base) | < 5 por registro (suspenso) |

A escritora é o gargalo: o mapeamento é 20 vezes mais rápido que a gravação, por isso o paralelismo não muda o total.

**Linha de base das alocações** (decisão de Paulo, 2026-11-13): as 141 alocações por registro ficam registradas como linha de base, e o critério de menos de 5 do doc 03 fica suspenso. O mapeamento custa cerca de 5% do tempo da importação, então reescrevê-lo agora seria otimização prematura. Arena e *interning* (doc 03, §2.2 item 5) só entram quando um perfil de memória ou de CPU mostrar necessidade real.

## 5. Casos de borda

1. Arquivo vazio ou só com espaços e quebras de linha no fim: relatório zerado.
2. Registro maior que o limite: relatado; a leitura recomeça no próximo terminador.
3. Arquivo terminado no meio de um registro: o último é relatado como truncado.
4. Lote menor que o número de threads: as threads que sobram não são criadas.

## 6. Fora do escopo

- Casamento com registros existentes e reimportação sem duplicar: tarefa 1.10.
- Linha de comando, MARCXML e UNIMARC na importação: tarefas 1.11 e Fase 4.
- Acelerar a escritora (por exemplo, não serializar os bytes do registro de origem como lista JSON no `audit_log`): quando um caso real pedir.
