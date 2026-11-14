# Grade de exemplares (v0)

> Status: v0, tarefa 2.7. Base: [spec de interação](../../UI_BIBFRAME_INTERACTION_SPEC.md) §5 e §9, [modelo bibliográfico](modelo-bibliografico.md), [mapeamento MARC21](mapeamento-marc21.md) §1.4 e [edição](edicao.md). Critério da tarefa: **50 exemplares bipados seguidos sem perder leitura; recusa por célula**.

A tarefa 2.7 tem duas partes, cada uma num PR:

- **2.7a, escrita:** o comando `catalog.save_items`, a validação por linha e a rota que grava o lote (itens 1 a 11 e 13 a 16).
- **2.7b, tela:** a leitura da grade (item 12) e a grade `<elz-item-grid>` na coluna de exemplares, com teclado, bipe, `Ctrl+D` e "+N linhas" (itens 17 a 29).

Ficam para depois: filtrar e ordenar dentro da grade, desenhar só as linhas visíveis (item 27), colunas visíveis e largura salvas por usuário (§5), a circulação rápida (Fase 3) e apagar exemplar (que no Koha tem regras próprias: empréstimo, reserva, último exemplar).

## 1. Comportamento

### O que a grade edita

1. Cada linha é um exemplar da Instância. As colunas editáveis são as do cadastro: código de barras, tombo, nº de chamada, unidade de origem, unidade atual, localização, coleção, tipo de item, nº do exemplar e preço. A situação (disponível, emprestado, extraviado...) aparece como sinal e **não** se edita na grade: é da circulação (Fase 3).
2. Preço de reposição, data de aquisição e os números de situação de um exemplar existente ficam como estão. Exemplar novo nasce com situação normal (zeros) e sem esses campos.

### Comando

3. `catalog.save_items` (`CatalogCommand::SaveItems`) grava o lote de uma Instância: linhas com `id` mudam aquele exemplar; linhas sem `id` criam exemplares novos, na ordem em que vêm.
4. O lote cita o `row_version` da Instância lido quando a grade abriu, como a edição ([edição](edicao.md), item 6). Versão diferente: `CatalogRefusal::Stale`, nada gravado.
5. **Atômico:** ou todas as linhas entram ou nenhuma. As recusas vêm todas juntas, uma por célula, em `CatalogRefusal::ItemRows`, com a posição da linha no lote, a coluna e o motivo:
   - unidade (de origem ou atual) desconhecida;
   - tipo de item desconhecido;
   - código de barras já usado por outro exemplar (da base ou de outra linha do mesmo lote);
   - exemplar (`id`) que não existe ou é de outra Instância.
6. Trocar códigos de barras entre dois exemplares do mesmo lote é permitido: a unicidade vale para o resultado do lote, não para a ordem das linhas.
7. Localização e coleção são termos de vocabulário, como na importação: um código novo cria o termo.
8. Lote aceito sobe uma vez o `row_version` da Instância e a reindexa (o índice mostra os exemplares). A resposta traz a nova versão e os ids de todas as linhas, na ordem do lote, para a grade continuar editando sem reler.
9. Lote vazio: aceito se a versão confere, sem gravar nada.

### Validação antes do comando

10. Uma função pura da camada app (`items.rs`) confere e normaliza as linhas antes do comando, e também recusa por célula, tudo de uma vez:
    - espaços nas pontas saem; texto vazio vira campo vazio;
    - unidade de origem e tipo de item são obrigatórios; unidade atual vazia vira a de origem;
    - preço aceita `12,50`, `12.50` ou `12` e vira centavos; outro formato é recusado;
    - código de barras repetido dentro do lote é recusado nas duas linhas.
11. O lote tem no máximo 500 linhas (limite explícito de entrada).

### Rotas

12. `GET /equipe/acervo/instancia/{id}/exemplares` devolve a grade (fragmento HTML) com as linhas gravadas, a versão e as listas de unidades e tipos de item para as células de escolha. Pede sessão e a permissão `cataloguing` (sem ela: 403; a leitura dos exemplares sem edição continua no bloco, listagem item 42).
13. `POST /equipe/acervo/instancia/{id}/exemplares` grava o lote. Corpo: `{"version": n, "rows": [{"id": 12 | null, "barcode": "...", ...}]}`. Mesma segurança da edição: sessão (401), CSRF e permissão (403), quem gravou no `audit_log`.
14. Aceito: 200 com `{"version": n, "ids": [...]}`. Recusado na validação ou no comando: 422 com `{"errors": [{"row": 3, "field": "barcode", "message": "..."}]}`. Versão velha: 409 com `{"current": n}`.
15. Os textos das recusas vêm do Fluent, como na edição.
16. A exportação MARC já trata exemplares: linha alterada mantém os subcampos do Koha do `952` com o mesmo código de barras; linha nova vira `952` novo ([mapeamento](mapeamento-marc21.md), §1.4).

### Tela (2.7b)

17. Quem cataloga vê "Editar exemplares" no bloco da Instância focada. A grade abre na coluna de exemplares, ancorada à Instância, que fica de relance à esquerda (§5).
18. A grade segue o padrão *grid* do WAI-ARIA, com `tabindex` itinerante: setas movem a célula, `Tab`/`Shift+Tab` andam na linha, `Enter` confirma e desce, `F2` ou digitar edita, `Esc` cancela a célula e um segundo `Esc` fecha a grade (perguntando se há mudança não gravada).
19. **Bipe:** o leitor de código de barras funciona como teclado. Na coluna "código de barras", `Enter` confirma e desce; na última linha, cria uma linha nova que herda unidade de origem, unidade atual, localização, coleção, tipo de item e nº de chamada da anterior, já com o cursor no código de barras. Assim 50 bipes seguidos viram 50 linhas, sem perder leitura. Se o bipe acabar e a pessoa apertar `Esc` na linha que esperava o próximo, essa linha some; uma linha que ficou esperando bipe e não recebeu nada não é gravada.
20. Nenhuma tecla vai ao servidor: a grade só grava com `Ctrl+Enter` ou o botão "Gravar".
21. `Ctrl+D` copia para a célula o valor da célula de cima.
22. "+N linhas" acrescenta N linhas herdando da última, com tombo inicial e incremento opcionais (`2026/0001` com incremento 1 gera `2026/0001` a `2026/00NN`: o último número do texto sobe, com os zeros à esquerda).
23. Linha nova ou alterada fica marcada até ser gravada. Recusa aparece na célula (item 5), com a mensagem, e o foco vai para a primeira.
24. Gravar com sucesso atualiza a versão e os ids e tira as marcas; a família é relida para as pílulas mostrarem os exemplares.
25. Sair da grade, trocar o foco ou fechar a família com mudança não gravada pergunta antes ([edição](edicao.md), item 32).
26. Unidade e tipo de item são escolhidos numa lista com os nomes cadastrados; localização e coleção são texto livre com sugestões dos nomes já usados. Digitar o nome ou o código (sem diferença de maiúsculas) escolhe o código; outro texto fica como digitado e o comando o confere (item 5).
27. A grade desenha todas as linhas. Desenhar só as visíveis acima de 200 linhas (§11) fica para quando uma medição mostrar a necessidade. Medido no Chromium (servidor local, 2026-11-14): acrescentar 250 linhas leva 89 ms, gravá-las 113 ms, reabrir a grade com 252 linhas 128 ms, e 100 setas seguidas 614 ms (cerca de 6 ms por tecla, contando o próprio Playwright).
28. A grade é um *Web Component* (`<elz-item-grid>`) em JavaScript sem dependências, no mesmo módulo da listagem.
29. **Nomes, não códigos.** Unidade, tipo de item, localização e coleção são gravados como código, mas toda tela mostra o nome da tabela correspondente (`branches.name`, `item_types.description`, `terms.label`): a grade, as pílulas da listagem, a ficha de campos e o OPAC. Código sem nome na tabela aparece como está. Na grade, a célula mostra o nome e guarda o código, que é o que o lote leva. Os nomes vêm do Koha e se editam numa tela própria (tarefa seguinte); como o OPAC guarda a página por versão do registro, renomear um código terá de invalidar essas páginas.

## 2. Origem no Koha

- Inclusão e edição de exemplar: `cataloguing/additem.pl` (`C4::Items::AddItemFromMarc`, `ModItemFromMarc`; `Koha::Item->store`).
- Lote: `tools/batchMod.pl` (edição em lote) e "Add multiple copies" de `additem.pl`, que incrementa o código de barras (`C4::Barcodes`).
- Código de barras único: restrição `itembarcodeidx` em `items.barcode`.
- Permissão: `editcatalogue` (`edit_items`).

## 3. Paridade

`livre`: a grade é interface. A paridade exigida continua a da exportação MARC (`952`), que o item 16 preserva. Diferença intencional: o Koha grava exemplar por exemplar; o Elzevir grava o lote inteiro ou nada.

Testes: `crates/elzevir-app/src/items.rs` (validação), `crates/elzevir-store/tests/items.rs` (comando, atomicidade, troca de códigos, versão, exportação) e `crates/elzevir-web/tests/staff_catalog.rs` (rotas).
