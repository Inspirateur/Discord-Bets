# Discord Bets
A discord bot that implements bets similar to *Twitch Bets*.

## Features

Users need to first make an account with `/make_account`  
This will create an account thread for the user, displaying the current balance and holding the history of transactions  
![account thread](https://github.com/Inspirateur/Discord-Bets/blob/main/pictures/balance.png)

A bet is created with `/bet desc [outcomes, ]` and accepts 2 outcomes or more, separated with spaces  
(quotes allows you to use spaces inside an option)  
![bet command](https://github.com/Inspirateur/Discord-Bets/blob/main/pictures/create_bet.png)

The bet will then be displayed like so, with informations on odds, amounts and users on each side, similar to *Twitch Bets*  
Users can bet on one outcome with the 10%, 50% and All in buttons (clicking multiple time on the same option is possible)  
![bet display](https://github.com/Inspirateur/Discord-Bets/blob/main/pictures/bet.png)

The creator of the bet can Abort it any time, or Lock it to close bidding while the action happens, 
which will remove the betting option and display win buttons to select the winning outcome  
![locked bet](https://github.com/Inspirateur/Discord-Bets/blob/main/pictures/lock.png)

When the bet has been settled, the creator of the bet can then select the winning option to distributes the gain among the winners  
![bet is over](https://github.com/Inspirateur/Discord-Bets/blob/main/pictures/win.png)

⚠️ *NOTICE: Due to thread renaming being slow, betting buttons might wrongly display `Interaction Failed.` sometimes  
Unless there's a server side error, this is not true, it just means that it's taking time.*

## How to run it
- Either grab a build from the releases or build it yourself, and put the executable in a folder
- go to https://discordapp.com/developers/applications/ create your app
  - add a User Bot to it, its token must be stored in an environnement variable named "GOTOH_TOKEN" on the computer running the bot
  - enable `SERVER MEMBERS INTENT` and `PRESENCE INTENT` in the bot tab  
  - invite the bot with `https://discord.com/api/oauth2/authorize?client_id=CLIENT_ID&permissions=0&scope=bot` replace `CLIENT_ID` with the Client ID of your app
- run the executable
