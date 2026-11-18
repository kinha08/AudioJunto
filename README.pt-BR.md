# AudioJunto

**Um som. Várias conexões.**

[English](README.md) · [Baixar](https://github.com/kinha08/AudioJunto/releases/latest) · [Relatar um problema](https://github.com/kinha08/AudioJunto/issues)

O AudioJunto é um aplicativo leve em Rust para **Windows 10 e 11 de 64 bits** que copia o áudio de uma saída para um ou mais dispositivos adicionais. Ouça junto usando dois fones ou compartilhe o som entre fones e caixas de som, com uma interface simples e controles na bandeja do sistema.

## Recursos

- Reproduz áudio em dois ou mais dispositivos, contando a saída original.
- Permite escolher a origem e várias saídas adicionais detectadas pelo Windows.
- Ativa e desativa a duplicação por um botão ou pelo menu da bandeja.
- Mantém o áudio original ao desligar a duplicação.
- Continua funcionando na bandeja ao fechar a janela.
- Salva os dispositivos selecionados; a duplicação sempre começa desligada.
- Mantém uma única instância por sessão do Windows. Abrir novamente restaura a janela existente.
- Utiliza as APIs de áudio do Windows, sem instalar driver virtual nem exigir administrador.

A interface e o instalador estão em português brasileiro.

## Download e instalação

Abra a [release mais recente](https://github.com/kinha08/AudioJunto/releases/latest) e escolha:

| Arquivo | Para que serve |
| --- | --- |
| `AudioJunto-Setup-0.1.3.exe` | Instalador com atalho no menu Iniciar, atalho opcional na área de trabalho e desinstalador. |
| `AudioJunto-0.1.3.exe` | Executável portátil. Coloque em uma pasta com permissão de gravação e execute. |
| `SHA256SUMS.txt` | Checksums para conferir a integridade dos downloads. |

A instalação é feita apenas para o usuário atual. As configurações ficam no arquivo `AudioJunto.ron`, ao lado do executável. Para desinstalar, use **Configurações → Aplicativos → Aplicativos instalados → AudioJunto**.

Se uma versão antiga estiver presa na bandeja, encerre o processo **AudioJunto** pelo Gerenciador de Tarefas antes de instalar a atualização.

## Como usar

1. Conecte os dois fones ou os demais dispositivos de saída.
2. Nas configurações de som do Windows, escolha o primeiro como saída normal e comece a reproduzir algum áudio.
3. Abra o AudioJunto e selecione esse dispositivo em **Origem do áudio**.
4. Em **Tocar também em**, marque as saídas adicionais.
5. Clique em **Ativar**. O dispositivo original continua tocando, enquanto o AudioJunto envia cópias para as saídas selecionadas.
6. Clique em **Desativar** para interromper as cópias e manter apenas o áudio original.

Desative a duplicação antes de mudar os dispositivos. Use **Atualizar** depois de conectar uma nova saída. Fechar a janela recolhe o aplicativo para a bandeja; dê dois cliques no ícone para reabrir. O menu do botão direito permite abrir a janela, ativar ou desativar a duplicação e **Sair**.

O AudioJunto não altera a saída padrão do Windows. Aplicativos configurados para tocar em outro dispositivo não entram na duplicação.

## Atraso e compatibilidade

A duplicação acrescenta o tempo de armazenamento em buffer e a latência da saída. O dispositivo original toca diretamente, por isso as cópias podem chegar um pouco depois. Fones Bluetooth podem acrescentar um atraso maior, e dispositivos diferentes podem não ficar perfeitamente sincronizados. Esta versão não oferece alinhamento manual do atraso da origem.

O aplicativo é voltado para ouvir junto. É menos adequado para monitoramento musical e outros usos que dependam de sincronização precisa. Conteúdo protegido, aplicativos em modo exclusivo e drivers incompatíveis podem impedir a captura ou a reprodução. Em fones Bluetooth, prefira o perfil estéreo ao perfil viva-voz.

Se uma saída falhar, o aplicativo interrompe a duplicação e informa o erro. Se a saída padrão do Windows mudar, a duplicação também para; confira a origem e ative novamente.

## Como funciona

O AudioJunto captura o áudio reproduzido na origem por **WASAPI loopback** e abre os destinos em modo compartilhado. Trabalha com amostras estéreo de ponto flutuante a 48 kHz, com conversão de formato pelo Windows. Cada saída possui uma fila limitada e interpolação adaptativa para compensar pequenas diferenças entre os relógios dos dispositivos.

O aplicativo não captura microfones, não grava áudio em arquivos e não transmite áudio pela rede.

## Compilar o projeto

Requisitos:

- Windows 10/11 x64.
- Rust estável com a toolchain `x86_64-pc-windows-msvc`.
- Visual Studio Build Tools com **Desenvolvimento para desktop com C++** e Windows SDK.

```powershell
git clone https://github.com/kinha08/AudioJunto.git
cd AudioJunto
cargo test --locked
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release --locked
.\target\release\audio-junto.exe
```

A vinculação estática do runtime C dispensa a instalação separada do Visual C++ Redistributable para o executável distribuído.

Para gerar o instalador, instale o [Inno Setup](https://jrsoftware.org/isinfo.php) com a tradução em português brasileiro e execute:

```powershell
Copy-Item target\release\audio-junto.exe installer\payload\AudioJunto.exe
New-Item -ItemType Directory -Force dist
& 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' "/O$PWD\dist" installer\AudioJunto.iss
```

Ajuste o caminho do compilador conforme sua instalação. O fluxo de release no GitHub Actions testa o código, compila o aplicativo e o instalador e publica os dois executáveis com seus checksums para uma nova versão na branch `main`.

## Testes e contribuições

Os testes automatizados cobrem os limites das filas de áudio, amostras estéreo, recuperação após silêncio, sinalização de instância única, inicializações simultâneas e notificações nativas para uma janela escondida. Eles não substituem testes auditivos em dispositivos reais.

Antes de enviar uma alteração, execute `cargo fmt --check` e `cargo test --locked`. Para mudanças no áudio ou na bandeja, teste também a reprodução, a desativação da duplicação, os comandos com a janela escondida, a desconexão de uma saída e a mudança da saída padrão do Windows. Ao [relatar um problema](https://github.com/kinha08/AudioJunto/issues), informe a versão do Windows, os tipos de dispositivos, a versão do aplicativo e os passos para reproduzir.

## Licença

[MIT](LICENSE) © 2026 Lucas Nobre.
