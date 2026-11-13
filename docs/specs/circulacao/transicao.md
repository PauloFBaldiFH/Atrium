# Máquina de estados da circulação

> Status: tarefa 1.12 (v0). Base: Koha 26.05.05. Primeira versão da função pura `transition()` (doc 03, §3.5): só o necessário para rodar os cenários de vencimento e multa.

## 1. Comportamento

1. Um exemplar está **disponível** ou **emprestado** (com o instante do vencimento).
2. Eventos: **empréstimo**, **renovação** e **devolução**, cada um com o instante em que acontece.
3. Empréstimo de exemplar disponível: fica emprestado, com o vencimento de [vencimento.md](vencimento.md), sem segundos.
4. Renovação de exemplar emprestado: o novo prazo (`renewalperiod`, ou `issuelength` se vazio) parte do vencimento atual ou do instante da renovação, conforme `ManualRenewalPeriodBase` (`date_due` ou `now`).
5. Devolução de exemplar emprestado: fica disponível. Com `CalculateFinesOnReturn`, a transição traz a multa de [multa.md](multa.md), contada do vencimento até a devolução; sem a preferência, não traz multa.
6. Recusas: empréstimo de exemplar que não está disponível; renovação ou devolução de exemplar que não está emprestado; vencimento impossível de calcular (nenhum dia aberto).
7. A função é pura: o mesmo estado, evento e contexto dão sempre o mesmo resultado. Regras e preferências entram já resolvidas.

## 2. Origem no Koha

- `C4::Circulation::AddIssue`, `AddRenewal` e `AddReturn` (`reference/koha/C4/Circulation.pm`), só a parte de vencimento e multa.

## 3. Paridade

`exigida` nas saídas de vencimento e multa; os cenários estão em [vencimento.md](vencimento.md) e [multa.md](multa.md).

## 4. Casos de borda

1. Renovar com base `now` um empréstimo que ainda não venceu: o prazo conta do instante da renovação.
2. Devolver sem `CalculateFinesOnReturn`: sem multa, mesmo com atraso.
3. Emprestar um exemplar já emprestado: recusa.

## 5. Desvios

Nenhum.

## 6. Fora do escopo (v0)

- Reservas, transferências, exemplar perdido ou retirado, limites de empréstimo e de renovação, bloqueios do leitor, débitos: cada um com sua tarefa.
