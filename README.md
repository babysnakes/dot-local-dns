## A Local Dns Server that Resolves "**.local" Addresses

This will be a small application that could be used together with a project like [Caddy Server][caddy] (web server that can easily be used for serving or proxying local directories and services) to avoid having to add addresses to your hosts file.

**Currently not usable**.

### Credits

* Big credit goes to [Emil Hernvall][emil] for his great [dnsguide][]. The entire DNS implementation is copied (with slight modifications) from his guide with his permission.
* [This github issue][issue391] for helping solve UDP connection resets in windows.

[caddy]: https://caddyserver.com/
[issue391]: https://github.com/mokeyish/smartdns-rs/issues/391
[emil]: https://github.com/EmilHernvall
[dnsguide]: https://github.com/EmilHernvall/dnsguide