// Permanent redirect from the old host to conzacut.conzahub.com, path and query kept.
export default {
  fetch(request) {
    const url = new URL(request.url);
    url.hostname = "conzacut.conzahub.com";
    return Response.redirect(url.toString(), 301);
  },
};
