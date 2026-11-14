# Listagem da equipe (v0)

> Status: v0, tarefa 2.3 (parte 1: leitura das famílias). Base: [spec de interação](../../UI_BIBFRAME_INTERACTION_SPEC.md) §2, §8 e §11 e [modelo bibliográfico](modelo-bibliografico.md). A tela (parte 2) e a medição com 1 milhão de registros (parte 3) vêm depois.

## 1. Comportamento

### Famílias

1. A listagem mostra **famílias**. Uma família é um Hub com todas as suas Obras, ou uma Obra fora de qualquer Hub. Obra de um Hub só aparece dentro da família do Hub.
2. Cada família traz:
   - o título do Hub, quando é um Hub;
   - cada Obra, por id, com título preferido, contribuidor principal (a entrada principal; sem ela, o primeiro contribuidor), classificação, idiomas e assuntos;
   - as Instâncias de cada Obra, por id, com versão do registro, título, edição, editora, data, primeiro ISBN e categoria;
   - os Itens de cada Instância.
3. A linha mostra até 3 assuntos por Obra e conta o resto ("+N"). Mostra até 4 pílulas de exemplar por Instância e conta o resto ("+N exemplares"). Mostra também o total de exemplares e quantos estão disponíveis.
4. Pílula: unidade, localização, número de chamada e tombo, com o sinal de balcão. Ordem: disponíveis primeiro, depois por tombo; empate mantém a ordem de criação.

### Sinal de balcão

5. O sinal de cada exemplar sai de `ItemStatus::signal`, nunca da tela:
   - **fora de circulação** (vermelho): `withdrawn`, `lost` ou `damaged` diferente de 0;
   - senão, **não emprestável** (cinza): `not_for_loan` ou `restricted` diferente de 0;
   - senão, **disponível** (verde).
6. Fora de circulação vence não emprestável: uma obra de referência perdida não está na estante. Emprestado (amarelo) e em trânsito ou reservado (azul) chegam com a circulação (Fase 3).

### Ordem e páginas

7. As famílias são ordenadas pela **chave de ordenação** do título do Hub ou da Obra: minúsculas, sem acentos, sem pontuação e com espaços únicos. Assim, "Ética", "ética" e "[Ética]" ficam juntas.
8. Empate na chave: Hub antes de Obra, e depois o id. A ordem é total, então uma página pode começar depois de qualquer família.
9. A paginação é por **cursor** (chave e família), 30 famílias por página, sem `OFFSET`. A página lê uma família a mais para saber se existe a próxima, sem contar o total.
10. A chave fica em `works.sort_key` e `hubs.sort_key` (migração 8, com índices parciais por chave e id). É recalculada ao criar Hub, acrescentar registro, substituir a descrição e editar o título da Obra. Bancos anteriores são preenchidos na migração.
11. Cada página é lida num número fixo de consultas ao pool de leitura: uma para as famílias e uma para cada parte (títulos de Hub, Obras, idiomas, assuntos, Instâncias e Itens), qualquer que seja o tamanho da página.

## 2. Origem no Koha

Não há equivalente direto. A lista de resultados da equipe do Koha (`catalogue/search.pl`) é por registro (`biblio`), não por família, e não tem listagem sem busca.

## 3. Paridade

`livre`: é interface.

Testes:
- `crates/elzevir-app/src/listing.rs`: chave de ordenação, sinal, pílulas e páginas;
- `crates/elzevir-store/tests/listing.rs`: 200 registros do oráculo em ordem, Hub e Obra com a mesma chave, renomear Obra, migração 8.

## 4. Fora desta parte

- Outras ordens (CDD, número de chamada, mais recentes) e busca agrupada por família: parte 2.
- Artigos iniciais ("O", "A", "The") ainda contam na ordenação. O MARC indica quantos caracteres pular (segundo indicador do 245), mas o modelo ainda não guarda isso.
