set -e
set -o pipefail

if [ -f "$HOME/.nvm/nvm.sh" ]; then
  source "$HOME/.nvm/nvm.sh"
else
  echo "nvm is not installed. Please install it first."
  echo "https://github.com/nvm-sh/nvm#readme"
  exit 1
fi
nvm install

yarn install
