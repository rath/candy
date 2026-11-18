// Evaluated in a rendered demo scene: returns where the recorded edits go, in image pixels.
// mosaic and highlight are [x, y, width, height] boxes around text; eyedropper is the box of
// the avatar whose colour the chat scene samples. render-scenes.sh saves the result beside
// each scene image, and record.swift drives the pointer from it.
(() => {
  const words = {
    en: { plan: "Saturday Oct 17, 7 PM at the fountain by the station", account: "mia.chen@example.com", phone: "(415) 555-0142" },
    ko: { plan: "10월 17일 토요일 저녁 7시, 합정역 7번 출구", account: "110-037-482916 김민지", phone: "010-0573-2918" },
    ja: { plan: "10月17日(土)の19時、恵比寿駅の西口前", account: "普通 1038472 スズキ ミサキ", phone: "090-0573-2918" },
  }[document.documentElement.lang];
  const k = window.devicePixelRatio;
  const box = (r) => [r.left, r.top, r.width, r.height].map((v) => Math.round(v * k));
  const union = (rects) => {
    const left = Math.min(...rects.map((r) => r.left));
    const top = Math.min(...rects.map((r) => r.top));
    const right = Math.max(...rects.map((r) => r.right));
    const bottom = Math.max(...rects.map((r) => r.bottom));
    return { left, top, right, bottom, width: right - left, height: bottom - top };
  };
  // The extent of an element's text, not of its box: cells and bubbles are wider than their words.
  const contents = (el) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    return union([...range.getClientRects()].filter((r) => r.width > 0));
  };
  const text = (needle) => {
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const at = node.data.indexOf(needle);
      if (at < 0) continue;
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + needle.length);
      return union([...range.getClientRects()]);
    }
    throw new Error(`text not found: ${needle}`);
  };
  const size = [document.documentElement.clientWidth * k, document.documentElement.clientHeight * k];

  if (document.querySelector(".feed")) {
    return {
      scene: "chat", size,
      mosaic: [box(text(words.account)), box(text(words.phone))],
      highlight: [box(text(words.plan))],
      eyedropper: box(document.querySelector(".msg .av").getBoundingClientRect()),
    };
  }
  if (document.querySelector("table")) {
    // Body rows 4-13 are the people; columns are th, No, Name, Team, Phone, Fee, Paid, Notes.
    const people = [...document.querySelectorAll("tbody tr")].slice(3, 13);
    const unpaid = people.filter((row) => row.querySelector("td.no"));
    return {
      scene: "sheet", size,
      mosaic: [box(union(people.map((row) => contents(row.children[4]))))],
      highlight: unpaid.map((row) => box(union([contents(row.children[2]), contents(row.children[6])]))),
    };
  }
  const rows = [...document.querySelectorAll(".card .row")];
  const account = document.querySelector(".account");
  const range = document.createRange();
  range.setStart(account.lastChild, 0);
  range.setEnd(account.lastChild, account.lastChild.length);
  return {
    scene: "order", size,
    mosaic: [box(union(rows.slice(2, 5).map((row) => contents(row.querySelector("dd"))))), box(range.getBoundingClientRect())],
    highlight: [box(contents(document.querySelector(".alert > div")))],
  };
})()
