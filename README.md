## A Local Dns Server that Resolves "**.loc" Addresses

This is a system tray icon application that resolves DNS queries of an illegal `*.loc` domain to either `localhost` or
to configured addresses. Currently, it only supports _Windows_ (I should add _macOS_ support soon).

> This app is very much a work-in-progress and everything can be changed on every release.

### What is it Used For?

Do any of these scenarios sound familiar?

* You need to serve directories via a local web browser.
* You're a web developer working on local projects.
* You have web services running on a NAS.
* You run services on Docker or Kubernetes on your machine.

In these cases, an easy-to-configure web server like [Caddy][] can help serve or proxy your websites—whether locally or
on your NAS. However, to access these sites conveniently, you’ll want to use domain names instead of IP:port
combinations, which means adding them to your hosts file.

You may also want to access other local services (e.g., NAS shares) by name rather than by IP address.

That's where DotLocal-DNS comes in. By default, it resolves any hostname ending in .loc (e.g., nas.loc) to
127.0.0.1. You can also configure specific hosts to resolve to custom IP addresses, making local access easier and more
intuitive.

### Usage

After [installing](#installation) the app and running it you probably want to perform 2 things:

* Click on the tray icon and toggle the _Startup at Login_ menu.
* [Configure your system](#configure-your-system-to-use-dotlocal-dns) to use _DotLocal-DNS_ to query hosts ending with
  _.loc_.

If you want to define custom addresses (e.g., to access your NAS) click the tray icon and select _Edit Records File_.
This will open the records text file - follow the instructions in the file for adding records.

### Installation

Check the instructions in the [Releases](https://github.com/babysnakes/dot-local-dns/releases) page and continue
to [configuring your system](#configure-your-system-to-use-dotlocal-dns).

### Configure Your System to Use DotLocal-DNS

Open _PowerShell_ console **as administrator** and run:

```powershell
Add-DnsClientNrptRule -Namespace ".loc" -NameServers "127.0.0.1"
```

To verify that the rule is accepted run:

```powershell
# list available rules
pwsh  Get-DnsClientNrptRule

Name                             : { EE27567A-76D5-4AF1-B446-A44CFCB1CC66 }
Version                          : 2
Namespace                        : { .loc }
IPsecCARestriction               :
DirectAccessDnsServers           :
DirectAccessEnabled              : False
DirectAccessProxyType            :
DirectAccessProxyName            :
DirectAccessQueryIPsecEncryption :
DirectAccessQueryIPsecRequired   :
NameServers                      : 127.0.0.1
DnsSecEnabled                    : False
DnsSecQueryIPsecEncryption       :
DnsSecQueryIPsecRequired         :
DnsSecValidationRequired         :
NameEncoding                     : Disable
DisplayName                      :
Comment                          :
```

---

If you want to remove the app run the following command:

```powershell
Get-DnsClientNrptRule | Where-Object { $_.Namespace -eq ".loc" } | Remove-DnsClientNrptRule
```

You should be prompted to approve deleting the rule. If something goes wrong you can run the `Get-nsClientNrptRule`
command as described above, Note the _Name_ and run as administrator:

```powershell
Remove-DnsClientNrptRule -Name "{EE27567A-76D5-4AF1-B446-A44CFCB1CC66}"
```

### Credits

* Big credit goes to [Emil Hernvall][emil] for his great [dnsguide][]. The entire DNS implementation is copied (with
  slight modifications) from his guide with his permission.
* [This GitHub issue][issue391] for helping solve UDP connection resets in windows.

[caddy]: https://caddyserver.com/

[issue391]: https://github.com/mokeyish/smartdns-rs/issues/391

[emil]: https://github.com/EmilHernvall

[dnsguide]: https://github.com/EmilHernvall/dnsguide