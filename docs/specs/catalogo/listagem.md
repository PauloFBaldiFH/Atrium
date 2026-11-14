# Listagem da equipe (v0)

> Status: v1. Tarefa 2.3; a §1.1 (lista compacta e foco) é da tarefa 2.4, parte 1. Antes: v0, tarefa 2.3 (parte 1: leitura das famílias; parte 2: tela da equipe e busca). Base: [spec de interação](../../UI_BIBFRAME_INTERACTION_SPEC.md) §2, §8 e §11 e [modelo bibliográfico](modelo-bibliografico.md). A medição com 1 milhão de registros (parte 3) vem depois.

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

### Busca na mesma tela

12. A busca filtra a mesma lista: com texto, as famílias saem das Instâncias que o índice encontrou (busca de texto, ISBN, CDD, número de chamada, código de barras e tombo, como no OPAC). Sem texto, é a listagem por título.
13. Os resultados são agrupados por família (o Hub, quando a Obra tem um; senão a Obra) e as famílias saem na ordem do seu melhor resultado. Na família, as Instâncias encontradas vêm primeiro na sua Obra e com destaque.
14. A página da busca lê 60 resultados e termina antes do resultado que abriria a 31ª família. A próxima página começa nesse resultado (`from`), então nenhum resultado é pulado. Uma família cujos resultados caem em duas páginas aparece nas duas.
15. A busca usa no máximo 256 caracteres. `from` acima de 10.000 é lido como 10.000.

### Tela

16. `GET /equipe/acervo` exige sessão da equipe. A página inicial da equipe tem o link.
17. Parâmetros: `q` (busca), `after` (cursor da listagem) e `from` (resultado inicial da busca). Cursor malformado ou com mais de 2.048 caracteres é lido como a primeira página.
18. A última linha da página carrega a próxima com HTMX quando aparece na tela (`hx-trigger="intersect once"`), e o servidor devolve só as linhas. Sem JavaScript, a mesma linha é um link para a próxima página.
19. O formulário de busca troca a lista inteira com HTMX e muda a URL; sem JavaScript, é um GET comum.
20. A pílula escreve o sinal por extenso, além da cor. Linhas fora da tela não são desenhadas (`content-visibility: auto`).
21. A tela segue o esboço v9: barra fixa com a busca e os botões de colunas, e fundo pontilhado. Cada família é uma grade de cinco trilhas (Autoridades, Hub, Obra, Instância, Exemplares), cada uma com o seu cabeçalho. A trilha do Hub some quando a família não tem Hub. Instância e Exemplares dividem as mesmas linhas (`subgrid`), então os exemplares ficam ao lado da sua Instância.
21a. Os botões "Authorities" e "Hub" ocultam e mostram essas colunas em todas as famílias (`aria-pressed`). A escolha é de cada pessoa e fica no navegador (`localStorage`); sem JavaScript os botões não aparecem e todas as colunas ficam visíveis. Ocultar uma coluna oculta também as cordas que saem dela.
22. Cada bloco é um cartão com selo e cor do seu tipo ("Agente", "Obra • por", "Instância • 2. ed."), com os campos rotulados: nome autorizado no Agente; título uniforme no Hub; título da obra, agentes, classificação e assuntos na Obra; capa, título, imprenta, ISBN e material na Instância. A Instância encontrada pela busca tem contorno destacado. Na coluna de Autoridades, por enquanto só leitura, aparece o contribuidor principal de cada Obra, uma vez por família. A capa ainda é um marcador colorido com título e ISBN (capas reais: §6.4).
23. Os exemplares são faixas empilhadas ao lado da sua Instância, uma por exemplar, com fundo e borda da cor do sinal. Cada faixa começa com o ícone do tipo de material da Instância (livro, periódico, mapa, partitura, som, vídeo, digital, material visual, misto), na cor do sinal, seguido do nome do sinal por extenso; por isso a barra não tem legenda. Os ícones vêm de `/static/elzevir-icons.svg`. Um último item "+N exemplares — ver todos" leva ao registro.
24. Cada cartão e cada faixa dizem quais são os seus pais (`data-node`; `data-parent` com os pais separados por espaço) e o rótulo de cada ligação, na mesma ordem e separados por `|`, escritos pelo servidor a partir do Fluent ("written by", "work", "edition", "items"). O módulo `elzevir-catalog.js` desenha, num `<svg>` sobreposto que não captura cliques, uma curva Bézier da borda direita de cada pai até cada filho: tracejada e roxa quando o pai é uma autoridade, cheia nas demais. Uma corda nunca passa por trás de um bloco: entre colunas vizinhas ela fica no vão, e quando pula uma coluna (autoridade direto para a Obra, passando pelo Hub) atravessa essa coluna numa faixa livre acima, entre ou abaixo dos blocos, a mais próxima do destino. O rótulo aparece uma vez por pai, sobre um fundo, no meio da curva ou no trecho que atravessa a coluna, quando há espaço. As curvas são refeitas por `ResizeObserver` e para as linhas que o HTMX acrescenta, e todas as posições são lidas antes de qualquer escrita.
25. Em tela estreita as colunas se empilham, o cabeçalho some e as curvas não são desenhadas. Não há `<script>` nem `style` inline, porque a CSP proíbe. O foco 80/20 e a trilha são da tarefa 2.4; criar e ligar autoridades, da 2.9; capas (§6.4), "+ Nova" (2.6) e os estados de circulação, depois.

### Desempenho

26. Medido com `cargo bench -p elzevir --bench opac`, que monta 1 milhão de registros (o export da equipe repetido) e passa pelo roteador inteiro, com sessão e renderização. Numa máquina de 4 núcleos, em 2026-11-14, com 2.000 páginas cada:
    - listagem, a partir de cursores espalhados pelo catálogo: p50 1,56 ms, p95 2,41 ms, p99 2,85 ms;
    - busca: p50 2,37 ms, p95 6,42 ms, p99 8,61 ms.

    O critério da tarefa é p95 abaixo de 50 ms no servidor.

### Lista compacta e foco (tarefa 2.4, parte 1)

27. Cada família aparece como uma **linha compacta** (spec de UI §20 item 2): ícone do material da primeira Instância, título (do Hub ou da Obra), o selo "Hub" quando é Hub, quantas Obras e Instâncias tem, os agentes como chips (cada nome uma vez), a classificação da primeira Obra e "N de M disponíveis" somando as Instâncias. Sem exemplares, diz que não há exemplares.
28. A linha é o `<summary>` de um `<details>`, e os blocos com cordas (itens 21 a 25) são o conteúdo dele. Abrir a linha é **focar a família**: sem JavaScript o navegador abre e fecha com clique, `Enter` ou espaço, e mais de uma pode ficar aberta.
29. Com o módulo `elzevir-catalog.js`, só uma família fica em foco: abrir uma fecha a anterior. A URL guarda o foco no fragmento (`#f-w40`, `#f-h3`, o mesmo token do cursor sem a chave), então recarregar ou mandar o link reabre a família, se ela estiver na página. A trilha no alto mostra "Acervo › título da família" enquanto há foco.
30. Teclado, fora de campos de texto: `↑` e `↓` passam de linha em linha; `Enter` abre ou fecha; `Esc` fecha a família em foco e devolve o cursor à linha dela; `/` vai para a busca.
31. As cordas são desenhadas quando a família abre e refeitas quando ela muda de tamanho; família fechada não tem cordas.
32. A busca abre sozinha a primeira família que tem uma Instância encontrada.

## 2. Origem no Koha

Não há equivalente direto. A lista de resultados da equipe do Koha (`catalogue/search.pl`) é por registro (`biblio`), não por família, e não tem listagem sem busca.

## 3. Paridade

`livre`: é interface.

Testes:
- `crates/elzevir-app/src/listing.rs`: chave de ordenação, sinal, pílulas e páginas;
- `crates/elzevir-store/tests/listing.rs`: 200 registros do oráculo em ordem, Hub e Obra com a mesma chave, famílias por cabeça, renomear Obra, migração 8;
- `crates/elzevir-web/tests/staff_catalog.rs`: sessão, páginas, linhas para o HTMX, busca, cursor malformado, linha compacta e foco.

## 4. Fora desta parte

- Outras ordens (CDD, número de chamada, mais recentes).
- O bipe de código de barras abrindo a grade daquele exemplar: tarefa 2.7.
- Artigos iniciais ("O", "A", "The") ainda contam na ordenação. O MARC indica quantos caracteres pular (segundo indicador do 245), mas o modelo ainda não guarda isso.
